"""
Tests for ASM (Atomic Slots Migration) in RedisJSON.

These tests verify that JSON operations work correctly during cluster slot migrations,
particularly focusing on potential issues with shared string modifications during resharding.
"""

import time
import random
import threading
import json
from dataclasses import dataclass
from typing import Optional, Set
import re

from RLTest import Env
from includes import VALGRIND


# Slot table for generating keys that map to specific hash slots
# This table maps hash slots to specific strings that hash to those slots
slot_table = [
    "06S", "Qi", "5L5", "4Iu", "4gY", "460", "1Y7", "1LV", "0QG", "ru", "7Ok", "4ji", "4DE", "65n", "2JH", "I8",
    "F9", "SX", "7nF", "4KD", "4eh", "6PK", "2ke", "1Ng", "0Sv", "4L", "491", "4hX", "4Ft", "5C4", "2Hy", "09R",
    "021", "0cX", "4Xv", "6mU", "6Cy", "42R", "0Mt", "nF", "cv", "1Pe", "5kK", "6NI", "74L", "4UF", "0nh", "MZ",
    "2TJ", "0ai", "4ZG", "6od", "6AH", "40c", "0OE", "lw", "aG", "0Bu", "5iz", "6Lx", "5R7", "4Ww", "0lY", "Ok",
    "5n3", "4ks", "8YE", "7g", "2KR", "1nP", "714", "64t", "69D", "4Ho", "07I", "Ps", "2hN", "1ML", "4fC", "7CA",
]


@dataclass(frozen=True)
class SlotRange:
    """Represents a range of hash slots in the cluster."""
    start: int
    end: int

    @staticmethod
    def from_str(s: str):
        """Parse a slot range string like '0-8191' into a SlotRange object."""
        start, end = map(int, s.split("-"))
        assert 0 <= start <= end < 2**14, f"Invalid slot range: {start}-{end}"
        return SlotRange(start, end)


@dataclass
class ClusterNode:
    """Represents a node in the Redis cluster."""
    id: str
    ip: str
    port: int
    cport: int  # cluster bus port
    hostname: Optional[str]
    flags: Set[str]
    master: str  # Either this node's primary replica or '-'
    ping_sent: int
    pong_recv: int
    config_epoch: int
    link_state: bool  # True: connected, False: disconnected
    slots: Set[SlotRange]

    @staticmethod
    def from_str(s: str):
        """Parse a CLUSTER NODES line into a ClusterNode object."""
        # Format: <id> <ip:port@cport[,hostname]> <flags> <master> <ping-sent> <pong-recv> <config-epoch> <link-state> <slot-range> ...
        parts = s.split()
        node_id, addr, flags, master, ping_sent, pong_recv, config_epoch, link_state, *slots = parts
        match = re.match(r"^(?P<ip>[^:]+):(?P<port>\d+)@(?P<cport>\d+)(?:,(?P<hostname>.+))?$", addr)
        ip = match.group("ip")
        port = int(match.group("port"))
        cport = int(match.group("cport"))
        hostname = match.group("hostname")

        return ClusterNode(
            id=node_id,
            ip=ip,
            port=port,
            cport=cport,
            hostname=hostname,
            flags=set(flags.split(",")),
            master=master,
            ping_sent=int(ping_sent),
            pong_recv=int(pong_recv),
            config_epoch=int(config_epoch),
            link_state=link_state == "connected",
            slots={SlotRange.from_str(s) for s in slots},
        )


def test_asm_without_data():
    """Test basic slot migration without any data - verifies the migration mechanism works."""
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    migrate_slots_back_and_forth(env)


def test_asm_with_json_data():
    """Test slot migration with JSON data present - ensures data integrity during migration."""
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    # Fill cluster with JSON documents
    fill_json_data(env, number_of_keys=100, nested_depth=3)
    
    # Perform migrations and verify data integrity
    migrate_slots_back_and_forth(env)


def test_asm_with_json_set_operations():
    """
    Test slot migration with concurrent JSON.SET operations.
    This is critical for detecting issues with shared string modifications.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    number_of_keys = 1000 if not VALGRIND else 100
    
    # Pre-populate with JSON data
    fill_json_data(env, number_of_keys, nested_depth=2)
    
    conn = env.getConnection(0)
    done = False
    errors = []

    def continuously_set_json():
        """Continuously modify JSON documents while migration happens."""
        counter = 0
        while not done:
            try:
                # Pick a random key
                hslot = random.randint(0, len(slot_table) - 1)
                key = f"json:{{{slot_table[hslot]}}}"
                
                # Perform various SET operations
                counter += 1
                doc = {
                    "counter": counter,
                    "type": "set_test",
                    "nested": {
                        "value": random.randint(0, 1000),
                        "string": f"test_{counter}"
                    }
                }
                conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
                
                # Verify we can read it back
                result = conn.execute_command("JSON.GET", key, "$")
                if result is None:
                    errors.append(f"Failed to get key {key} after setting it")
                    
            except Exception as e:
                # ASK/MOVED errors are expected during migration
                if "MOVED" not in str(e) and "ASK" not in str(e):
                    errors.append(f"Unexpected error: {e}")

    thread = threading.Thread(target=continuously_set_json)
    thread.start()

    # Perform migrations while SET operations are running
    migrate_slots_back_and_forth(env)

    done = True
    thread.join()

    # Check for any unexpected errors
    if errors:
        raise AssertionError(f"Encountered {len(errors)} errors: {errors[:10]}")


def test_asm_with_json_array_operations():
    """
    Test slot migration with concurrent array modifications.
    Array operations are particularly sensitive to shared string issues.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    number_of_keys = 500 if not VALGRIND else 50
    
    # Pre-populate with JSON arrays
    with env.getClusterConnectionIfNeeded() as conn:
        for i in range(number_of_keys):
            hslot = i * (len(slot_table) - 1) // (number_of_keys - 1)
            key = f"json:{{{slot_table[hslot]}}}"
            initial_array = [{"id": j, "value": f"item_{j}"} for j in range(10)]
            conn.execute_command("JSON.SET", key, "$", json.dumps(initial_array))

    done = False
    errors = []

    def continuously_modify_arrays():
        """Continuously append to and modify arrays during migration."""
        # Each thread needs its own cluster connection
        with env.getClusterConnectionIfNeeded() as thread_conn:
            counter = 0
            while not done:
                try:
                    hslot = random.randint(0, len(slot_table) - 1)
                    key = f"json:{{{slot_table[hslot]}}}"
                    
                    counter += 1
                    # Append to array
                    new_item = json.dumps({"id": counter, "value": f"appended_{counter}"})
                    thread_conn.execute_command("JSON.ARRAPPEND", key, "$", new_item)
                    
                    # Get array length
                    length = thread_conn.execute_command("JSON.ARRLEN", key, "$")
                    if length and length[0] < 10:
                        errors.append(f"Array length {length[0]} is less than initial size")
                        
                except Exception as e:
                    if "MOVED" not in str(e) and "ASK" not in str(e) and "ERR" not in str(e):
                        errors.append(f"Unexpected error: {e}")

    thread = threading.Thread(target=continuously_modify_arrays)
    thread.start()

    migrate_slots_back_and_forth(env)

    done = True
    thread.join()

    if errors:
        raise AssertionError(f"Encountered {len(errors)} errors: {errors[:10]}")


def test_asm_with_json_string_modifications():
    """
    Test slot migration with string value modifications in JSON documents.
    This specifically targets the shared string concern mentioned by the user.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    number_of_keys = 500 if not VALGRIND else 50
    
    # Create documents with string fields
    with env.getClusterConnectionIfNeeded() as conn:
        for i in range(number_of_keys):
            hslot = i * (len(slot_table) - 1) // (number_of_keys - 1)
            key = f"json:{{{slot_table[hslot]}}}"
            doc = {
                "id": i,
                "name": f"user_{i}",
                "description": f"This is a test document for user {i}",
                "tags": ["tag1", "tag2", "tag3"],
                "metadata": {
                    "created": "2024-01-01",
                    "modified": "2024-01-01"
                }
            }
            conn.execute_command("JSON.SET", key, "$", json.dumps(doc))

    done = False
    errors = []
    modification_count = [0]  # Use list to allow modification in nested function

    def continuously_modify_strings():
        """Modify string fields in JSON documents during migration."""
        # Each thread needs its own cluster connection
        with env.getClusterConnectionIfNeeded() as thread_conn:
            while not done:
                try:
                    hslot = random.randint(0, len(slot_table) - 1)
                    key = f"json:{{{slot_table[hslot]}}}"
                    
                    # Modify various string fields
                    modification_count[0] += 1
                    new_desc = f"Modified description {modification_count[0]}"
                    thread_conn.execute_command("JSON.SET", key, "$.description", json.dumps(new_desc))
                    
                    # Verify the modification
                    result = thread_conn.execute_command("JSON.GET", key, "$.description")
                    if result:
                        retrieved = json.loads(result)
                        if retrieved[0] != new_desc:
                            errors.append(f"String mismatch: expected {new_desc}, got {retrieved[0]}")
                            
                except Exception as e:
                    if "MOVED" not in str(e) and "ASK" not in str(e):
                        errors.append(f"Unexpected error: {e}")

    thread = threading.Thread(target=continuously_modify_strings)
    thread.start()

    migrate_slots_back_and_forth(env)

    done = True
    thread.join()

    env.debugPrint(f"Total modifications during migration: {modification_count[0]}", force=True)
    
    if errors:
        raise AssertionError(f"Encountered {len(errors)} errors: {errors[:10]}")


def test_asm_with_mixed_json_operations():
    """
    Test slot migration with a mix of JSON operations: GET, SET, DEL, ARRAPPEND, OBJKEYS, etc.
    This simulates realistic workload during migration.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    number_of_keys = 500 if not VALGRIND else 50
    
    # Pre-populate
    fill_json_data(env, number_of_keys, nested_depth=2)
    
    conn = env.getConnection(0)
    done = False
    errors = []
    operation_counts = {"get": 0, "set": 0, "del": 0, "arrappend": 0, "objkeys": 0}

    def mixed_operations():
        """Perform a variety of JSON operations during migration."""
        while not done:
            try:
                hslot = random.randint(0, len(slot_table) - 1)
                key = f"json:{{{slot_table[hslot]}}}"
                
                # Randomly choose an operation
                op = random.choice(["get", "set", "del", "arrappend", "objkeys"])
                operation_counts[op] += 1
                
                if op == "get":
                    conn.execute_command("JSON.GET", key, "$")
                elif op == "set":
                    doc = {"value": random.randint(0, 1000), "op": "set"}
                    conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
                elif op == "del":
                    # Delete and recreate
                    conn.execute_command("DEL", key)
                    doc = {"value": random.randint(0, 1000), "op": "recreate"}
                    conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
                elif op == "arrappend":
                    # Set as array if needed, then append
                    conn.execute_command("JSON.SET", key, "$", json.dumps([1, 2, 3]))
                    conn.execute_command("JSON.ARRAPPEND", key, "$", "4", "5")
                elif op == "objkeys":
                    doc = {"a": 1, "b": 2, "c": 3}
                    conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
                    conn.execute_command("JSON.OBJKEYS", key, "$")
                    
            except Exception as e:
                if "MOVED" not in str(e) and "ASK" not in str(e) and "WRONGTYPE" not in str(e):
                    errors.append(f"Unexpected error in {op}: {e}")

    thread = threading.Thread(target=mixed_operations)
    thread.start()

    migrate_slots_back_and_forth(env)

    done = True
    thread.join()

    env.debugPrint(f"Operation counts: {operation_counts}", force=True)
    
    if errors:
        raise AssertionError(f"Encountered {len(errors)} errors: {errors[:10]}")


def test_asm_with_large_json_documents():
    """
    Test migration with large JSON documents.
    Large documents may have more complex memory management with shared strings.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    number_of_keys = 100 if not VALGRIND else 10
    
    # Create large documents
    with env.getClusterConnectionIfNeeded() as conn:
        for i in range(number_of_keys):
            hslot = i * (len(slot_table) - 1) // (number_of_keys - 1)
            key = f"json:{{{slot_table[hslot]}}}"
            
            # Create a large nested document
            large_doc = {
                "id": i,
                "data": [
                    {
                        "field": f"value_{j}",
                        "nested": {
                            "deep": {
                                "value": f"deep_value_{j}",
                                "array": list(range(10))
                            }
                        }
                    } for j in range(50)
                ],
                "metadata": {
                    "description": "A" * 1000,  # Large string
                    "tags": [f"tag_{k}" for k in range(100)]
                }
            }
            conn.execute_command("JSON.SET", key, "$", json.dumps(large_doc))

        # Verify data before migration
        test_key = f"json:{{{slot_table[0]}}}"
        original_data = conn.execute_command("JSON.GET", test_key, "$")

    # Perform migration
    migrate_slots_back_and_forth(env)

    # Verify data after migration
    with env.getClusterConnectionIfNeeded() as conn:
        migrated_data = conn.execute_command("JSON.GET", test_key, "$")
        assert original_data == migrated_data, "Data corruption detected in large document after migration"


# Helper functions

def fill_json_data(env, number_of_keys: int, nested_depth: int = 2):
    """Fill the cluster with JSON documents distributed across slots."""
    with env.getClusterConnectionIfNeeded() as conn:
        for i in range(number_of_keys):
            # Distribute keys across hash slots
            hslot = i * (len(slot_table) - 1) // (number_of_keys - 1)
            key = f"json:{{{slot_table[hslot]}}}"
            
            # Create a nested document
            doc = create_nested_doc(i, nested_depth)
            conn.execute_command("JSON.SET", key, "$", json.dumps(doc))


def create_nested_doc(id_val: int, depth: int) -> dict:
    """Create a nested JSON document for testing."""
    if depth == 0:
        return {
            "id": id_val,
            "value": f"value_{id_val}",
            "number": random.randint(0, 1000)
        }
    
    return {
        "id": id_val,
        "level": depth,
        "data": f"data_at_level_{depth}",
        "array": [i for i in range(5)],
        "nested": create_nested_doc(id_val, depth - 1)
    }


def cluster_node_of(conn) -> ClusterNode:
    """Get the ClusterNode information for the node this connection is connected to."""
    for line in conn.execute_command("cluster", "nodes").splitlines():
        cluster_node = ClusterNode.from_str(line)
        if "myself" in cluster_node.flags:
            return cluster_node
    raise ValueError("No node with 'myself' flag found")


def middle_slot_range(slot_range: SlotRange) -> SlotRange:
    """Get the middle third of a slot range."""
    third = (slot_range.end - slot_range.start) // 3
    return SlotRange(slot_range.start + third, slot_range.end - third)


def cantorized_slot_set(slot_range: SlotRange) -> Set[SlotRange]:
    """
    Split a slot range by removing the middle third (like a Cantor set).
    Returns the remaining two ranges.
    """
    middle = middle_slot_range(slot_range)
    return {
        SlotRange(slot_range.start, middle.start - 1),
        SlotRange(middle.end + 1, slot_range.end)
    }


def migrate_slots_back_and_forth(env):
    """
    Perform slot migrations back and forth between two shards.
    This tests that data remains intact and accessible during and after migrations.
    """
    first_conn, second_conn = env.getConnection(0), env.getConnection(1)
    
    # Get original slot distributions
    original_first_slot_range, = cluster_node_of(first_conn).slots
    original_second_slot_range, = cluster_node_of(second_conn).slots
    middle_of_original_first = middle_slot_range(original_first_slot_range)
    middle_of_original_second = middle_slot_range(original_second_slot_range)

    # First migration: move slots from second to first
    import_slots(first_conn, middle_of_original_second)
    assert cluster_node_of(first_conn).slots == {original_first_slot_range, middle_of_original_second}
    assert cluster_node_of(second_conn).slots == cantorized_slot_set(original_second_slot_range)

    # Second migration: move slots back from first to second
    import_slots(second_conn, middle_of_original_second)
    assert cluster_node_of(first_conn).slots == {original_first_slot_range}
    assert cluster_node_of(second_conn).slots == {original_second_slot_range}

    # Third migration: move slots from first to second
    import_slots(second_conn, middle_of_original_first)
    assert cluster_node_of(second_conn).slots == {original_second_slot_range, middle_of_original_first}
    assert cluster_node_of(first_conn).slots == cantorized_slot_set(original_first_slot_range)

    # Fourth migration: move slots back from second to first
    import_slots(first_conn, middle_of_original_first)
    assert cluster_node_of(first_conn).slots == {original_first_slot_range}
    assert cluster_node_of(second_conn).slots == {original_second_slot_range}


def import_slots(conn, slot_range: SlotRange):
    """
    Import a range of slots to the node connected via conn.
    Waits for the migration to complete.
    """
    task_id = conn.execute_command("CLUSTER", "MIGRATION", "IMPORT", slot_range.start, slot_range.end)
    start_time = time.time()
    timeout = 5 if not VALGRIND else 60
    
    while time.time() - start_time < timeout:
        migration_status, = conn.execute_command("CLUSTER", "MIGRATION", "STATUS", "ID", task_id)
        migration_status = {
            key: value 
            for key, value in zip(migration_status[0::2], migration_status[1::2])
        }
        if migration_status["state"] == "completed":
            return
        time.sleep(0.1)
    
    raise TimeoutError(f"Migration did not complete within {timeout} seconds")

