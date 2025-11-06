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



# ============================================================================
# ASM STRESS TESTS - Tests from test_asm_stress.py
# ============================================================================
def test_asm_extreme_string_pressure():
    """
    Extreme stress test: Multiple threads hammering string-heavy JSON documents
    during continuous migrations to expose shared string cache race conditions.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    # Create many keys with LOTS of string data to pressure the shared string cache
    num_keys = 200 if not VALGRIND else 20
    strings_per_doc = 100  # Each document has 100 string fields
    
    env.debugPrint(f"Creating {num_keys} keys with {strings_per_doc} strings each...", force=True)
    
    with env.getClusterConnectionIfNeeded() as conn:
        for i in range(num_keys):
            hslot = i * (len(slot_table) - 1) // (num_keys - 1)
            key = f"json:{{{slot_table[hslot]}}}"
            
            # Create document with MANY string fields (pressure on shared string cache)
            doc = {
                f"string_field_{j}": f"This is string value {j} for key {i} with lots of text to make it bigger and pressure the cache"
                for j in range(strings_per_doc)
            }
            conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
    
    env.debugPrint("Starting extreme concurrent modification test...", force=True)
    
    done = False
    errors = []
    operation_counts = [0]
    crash_detected = [False]
    
    def hammer_strings_thread_1():
        """Thread 1: Continuously modify strings with new values"""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            counter = 0
            while not done and not crash_detected[0]:
                try:
                    # Pick random key and random field
                    key_idx = random.randint(0, num_keys - 1)
                    field_idx = random.randint(0, strings_per_doc - 1)
                    hslot = key_idx * (len(slot_table) - 1) // (num_keys - 1)
                    key = f"json:{{{slot_table[hslot]}}}"
                    
                    counter += 1
                    operation_counts[0] += 1
                    
                    # Modify the string (causes shared cache access)
                    new_value = f"MODIFIED by thread1 counter {counter} with more text to increase cache pressure"
                    thread_conn.execute_command("JSON.SET", key, f"$.string_field_{field_idx}", json.dumps(new_value))
                    
                    # No sleep - maximum pressure
                except Exception as e:
                    error_str = str(e)
                    if "MOVED" not in error_str and "ASK" not in error_str and "CLUSTERDOWN" not in error_str:
                        errors.append(f"Thread1 error: {e}")
                        if "crash" in error_str.lower() or "segfault" in error_str.lower():
                            crash_detected[0] = True

    def hammer_strings_thread_2():
        """Thread 2: Continuously replace entire documents"""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            counter = 0
            while not done and not crash_detected[0]:
                try:
                    key_idx = random.randint(0, num_keys - 1)
                    hslot = key_idx * (len(slot_table) - 1) // (num_keys - 1)
                    key = f"json:{{{slot_table[hslot]}}}"
                    
                    counter += 1
                    operation_counts[0] += 1
                    
                    # Replace entire document (lots of string cache operations)
                    doc = {
                        f"string_field_{j}": f"Thread2 replacement {counter}-{j} with lots of string data to pressure cache"
                        for j in range(strings_per_doc)
                    }
                    thread_conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
                    
                except Exception as e:
                    error_str = str(e)
                    if "MOVED" not in error_str and "ASK" not in error_str and "CLUSTERDOWN" not in error_str:
                        errors.append(f"Thread2 error: {e}")
                        if "crash" in error_str.lower() or "segfault" in error_str.lower():
                            crash_detected[0] = True

    def hammer_strings_thread_3():
        """Thread 3: Continuously read and verify strings"""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            while not done and not crash_detected[0]:
                try:
                    key_idx = random.randint(0, num_keys - 1)
                    hslot = key_idx * (len(slot_table) - 1) // (num_keys - 1)
                    key = f"json:{{{slot_table[hslot]}}}"
                    
                    operation_counts[0] += 1
                    
                    # Read entire document (accesses all strings in cache)
                    # Use explicit $ path to get JSONPath format (array)
                    result = thread_conn.execute_command("JSON.GET", key, "$")
                    if result:
                        try:
                            doc = json.loads(result)
                            # Verify structure (accessing strings from cache)
                            if not isinstance(doc, list) or len(doc) == 0:
                                errors.append(f"Thread3: Invalid document structure for {key}: type={type(doc).__name__}, repr={repr(doc)[:200]}")
                            elif isinstance(doc, list) and len(doc) > 0:
                                # Also check that the content is a dict with the expected fields
                                first_elem = doc[0]
                                if not isinstance(first_elem, dict):
                                    errors.append(f"Thread3: Document has non-dict element for {key}: {repr(first_elem)[:200]}")
                                elif not any(k.startswith("string_field_") for k in first_elem.keys()):
                                    errors.append(f"Thread3: Document missing string_field keys for {key}: {list(first_elem.keys())[:10]}")
                        except json.JSONDecodeError as jde:
                            errors.append(f"Thread3: JSON decode error for {key}: {jde}, raw={result[:200]}")
                    
                except Exception as e:
                    error_str = str(e)
                    if "MOVED" not in error_str and "ASK" not in error_str and "CLUSTERDOWN" not in error_str:
                        errors.append(f"Thread3 error: {e}")
                        if "crash" in error_str.lower() or "segfault" in error_str.lower():
                            crash_detected[0] = True

    # Start multiple aggressive threads
    threads = [
        threading.Thread(target=hammer_strings_thread_1),
        threading.Thread(target=hammer_strings_thread_2),
        threading.Thread(target=hammer_strings_thread_3),
        threading.Thread(target=hammer_strings_thread_1),  # Extra thread 1
        threading.Thread(target=hammer_strings_thread_2),  # Extra thread 2
    ]
    
    for t in threads:
        t.start()
    
    # Give threads time to start hammering
    time.sleep(0.5)
    
    env.debugPrint("Starting rapid-fire migrations...", force=True)
    
    # Perform MANY rapid migrations to maximize collision probability
    try:
        for migration_round in range(10):  # 10 back-and-forth cycles
            if crash_detected[0]:
                break
            env.debugPrint(f"Migration round {migration_round + 1}/10", force=True)
            migrate_slots_rapid(env)
            time.sleep(0.1)  # Brief pause between rounds
    finally:
        done = True
        for t in threads:
            t.join(timeout=5)
    
    env.debugPrint(f"Total operations: {operation_counts[0]}", force=True)
    env.debugPrint(f"Errors encountered: {len(errors)}", force=True)
    
    if crash_detected[0]:
        raise AssertionError("CRASH DETECTED during concurrent operations and migrations!")
    
    if errors:
        # Show first 20 unique errors
        unique_errors = list(set(errors))[:20]
        raise AssertionError(f"Encountered {len(errors)} errors ({len(unique_errors)} unique): {unique_errors}")


def test_asm_same_key_concurrent_modification():
    """
    Focus test: Multiple threads modifying the SAME keys that are being migrated.
    This maximizes the chance of hitting shared string cache from multiple threads.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    # Create keys that will be in the migration slot range
    target_slots = [slot_table[5], slot_table[10], slot_table[15]]  # Focus on specific slots
    
    with env.getClusterConnectionIfNeeded() as conn:
        for slot_key in target_slots:
            for i in range(50):  # 50 keys per slot
                key = f"json:{{{slot_key}}}:{i}"
                doc = {
                    "data": f"Initial value for {key}",
                    "counter": 0,
                    "large_string": "X" * 1000  # Large string to pressure cache
                }
                conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
    
    done = False
    errors = []
    modifications = [0]
    
    def modify_same_keys():
        """Hammer the same keys that are being migrated"""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            while not done:
                try:
                    slot_key = random.choice(target_slots)
                    key_num = random.randint(0, 49)
                    key = f"json:{{{slot_key}}}:{key_num}"
                    
                    modifications[0] += 1
                    
                    # Increment counter and modify string
                    result = thread_conn.execute_command("JSON.NUMINCRBY", key, "$.counter", 1)
                    new_data = f"Modified {modifications[0]} times"
                    thread_conn.execute_command("JSON.SET", key, "$.data", json.dumps(new_data))
                    
                except Exception as e:
                    if "MOVED" not in str(e) and "ASK" not in str(e):
                        errors.append(str(e))
    
    # Start multiple threads targeting the SAME keys
    threads = [threading.Thread(target=modify_same_keys) for _ in range(8)]
    for t in threads:
        t.start()
    
    time.sleep(0.3)
    
    # Migrate the slots that contain our target keys
    env.debugPrint("Starting migration of target slots...", force=True)
    migrate_slots_rapid(env)
    
    # Continue hammering for a bit
    time.sleep(0.5)
    
    done = True
    for t in threads:
        t.join(timeout=5)
    
    env.debugPrint(f"Total modifications: {modifications[0]}", force=True)
    
    if errors:
        raise AssertionError(f"Encountered {len(errors)} errors: {errors[:10]}")


def migrate_slots_rapid(env):
    """Perform rapid slot migrations to maximize race condition window."""
    first_conn, second_conn = env.getConnection(0), env.getConnection(1)
    
    def get_node_slots(conn):
        for line in conn.execute_command("cluster", "nodes").splitlines():
            node = ClusterNode.from_str(line)
            if "myself" in node.flags:
                return node.slots
        raise ValueError("No node with 'myself' flag found")
    
    def get_middle_range(slot_range: SlotRange) -> SlotRange:
        third = (slot_range.end - slot_range.start) // 3
        return SlotRange(slot_range.start + third, slot_range.end - third)
    
    original_first, = get_node_slots(first_conn)
    original_second, = get_node_slots(second_conn)
    middle_first = get_middle_range(original_first)
    middle_second = get_middle_range(original_second)
    
    # Migrate from second to first
    task_id = first_conn.execute_command("CLUSTER", "MIGRATION", "IMPORT", middle_second.start, middle_second.end)
    wait_for_migration(first_conn, task_id, timeout=3)
    
    # Migrate back
    task_id = second_conn.execute_command("CLUSTER", "MIGRATION", "IMPORT", middle_second.start, middle_second.end)
    wait_for_migration(second_conn, task_id, timeout=3)


def wait_for_migration(conn, task_id, timeout=5):
    """Wait for migration to complete."""
    start = time.time()
    while time.time() - start < timeout:
        status, = conn.execute_command("CLUSTER", "MIGRATION", "STATUS", "ID", task_id)
        status_dict = {key: value for key, value in zip(status[0::2], status[1::2])}
        if status_dict["state"] == "completed":
            return
        time.sleep(0.01)  # Very short sleep for rapid checking
    raise TimeoutError(f"Migration {task_id} did not complete")


def test_asm_use_after_free_crash():
    """
    Attempt to trigger use-after-free crash by accessing keys during TRIM phase.
    
    Scenario:
    1. Thread reads key, gets reference to shared strings
    2. Migration moves key to other shard
    3. TRIM phase frees the key and strings on source shard
    4. Thread tries to format response with freed strings
    5. Expected: CRASH (use-after-free)
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    num_keys = 100
    strings_per_doc = 50
    
    env.debugPrint(f"Creating {num_keys} keys with large strings...", force=True)
    
    with env.getClusterConnectionIfNeeded() as conn:
        for i in range(num_keys):
            hslot = i * (len(slot_table) - 1) // (num_keys - 1)
            key = f"json:{{{slot_table[hslot]}}}"
            
            # Create large documents with many strings (lots of cache entries)
            doc = {
                f"string_{j}": f"Large string value {j} for key {i} " + ("X" * 500)
                for j in range(strings_per_doc)
            }
            conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
    
    env.debugPrint("Starting use-after-free test...", force=True)
    
    done = False
    errors = []
    crashes = []
    reads_completed = [0]
    
    def aggressive_reader():
        """Continuously read keys, especially during migration"""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            while not done:
                try:
                    # Pick random key
                    key_idx = random.randint(0, num_keys - 1)
                    hslot = key_idx * (len(slot_table) - 1) // (num_keys - 1)
                    key = f"json:{{{slot_table[hslot]}}}"
                    
                    # Deep JSONPath query to force string access
                    result = thread_conn.execute_command("JSON.GET", key, "$")
                    
                    if result:
                        # Force parsing to actually access the strings
                        data = json.loads(result)
                        if data and len(data) > 0:
                            # Access nested strings to force cache usage
                            for item in data[:5]:  # Check first 5 items
                                if isinstance(item, str) and len(item) > 0:
                                    _ = item[0]  # Force string access
                    
                    reads_completed[0] += 1
                    
                    # No sleep - maximize race condition window
                    
                except Exception as e:
                    error_str = str(e)
                    if "MOVED" not in error_str and "ASK" not in error_str and "CLUSTERDOWN" not in error_str:
                        if "connection" in error_str.lower() or "broken pipe" in error_str.lower():
                            crashes.append(f"Connection error (possible crash): {e}")
                        else:
                            errors.append(str(e))
    
    def rapid_deep_queries():
        """Issue complex queries that traverse entire documents"""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            while not done:
                try:
                    key_idx = random.randint(0, num_keys - 1)
                    hslot = key_idx * (len(slot_table) - 1) // (num_keys - 1)
                    key = f"json:{{{slot_table[hslot]}}}"
                    
                    # Different query types to access strings in different ways
                    query_type = random.randint(0, 2)
                    if query_type == 0:
                        # Get entire document (accesses all strings)
                        result = thread_conn.execute_command("JSON.GET", key, "$")
                        if result:
                            data = json.loads(result)
                            # Access all string values
                            if data and isinstance(data, list) and len(data) > 0:
                                for val in list(data[0].values())[:10]:
                                    _ = str(val)
                    elif query_type == 1:
                        # Get specific fields
                        thread_conn.execute_command("JSON.GET", key, "$.string_10")
                        thread_conn.execute_command("JSON.GET", key, "$.string_20")
                    else:
                        # Get entire document with explicit $ path
                        result = thread_conn.execute_command("JSON.GET", key, "$")
                        if result:
                            _ = len(result)  # Force string access
                    
                    reads_completed[0] += 1
                    
                except Exception as e:
                    error_str = str(e)
                    if "MOVED" not in error_str and "ASK" not in error_str and "CLUSTERDOWN" not in error_str:
                        if "connection" in error_str.lower() or "broken pipe" in error_str.lower():
                            crashes.append(f"Connection error (possible crash): {e}")
                        else:
                            errors.append(str(e))
    
    # Start many reader threads
    threads = [
        threading.Thread(target=aggressive_reader),
        threading.Thread(target=aggressive_reader),
        threading.Thread(target=rapid_deep_queries),
        threading.Thread(target=rapid_deep_queries),
        threading.Thread(target=aggressive_reader),
        threading.Thread(target=rapid_deep_queries),
    ]
    
    for t in threads:
        t.start()
    
    # Let readers get going
    time.sleep(0.2)
    
    env.debugPrint("Starting migrations while threads are reading...", force=True)
    
    # Perform multiple rapid migrations while threads are actively reading
    try:
        for round_num in range(8):
            env.debugPrint(f"Migration round {round_num + 1}/8 (reads so far: {reads_completed[0]})", force=True)
            
            # Migrate and immediately continue (don't wait long)
            migrate_slots_rapid(env)
            
            # Very short pause - maximize chance of use-after-free
            time.sleep(0.05)
            
            # Check if Redis crashed
            try:
                with env.getClusterConnectionIfNeeded() as conn:
                    conn.execute_command("PING")
            except Exception as e:
                crashes.append(f"Redis shard appears to have crashed: {e}")
                break
                
    finally:
        done = True
        for t in threads:
            t.join(timeout=5)
    
    env.debugPrint(f"Total reads completed: {reads_completed[0]}", force=True)
    env.debugPrint(f"Crashes detected: {len(crashes)}", force=True)
    env.debugPrint(f"Errors: {len(errors)}", force=True)
    
    if crashes:
        raise AssertionError(f"CRASH DETECTED! {len(crashes)} crashes: {crashes[:5]}")
    
    if errors:
        # Show unique errors
        unique_errors = list(set(errors))[:10]
        env.debugPrint(f"Non-crash errors: {unique_errors}", force=True)


def test_asm_read_during_trim():
    """
    Try to read keys at the exact moment they're being trimmed from source shard.
    
    The TRIM phase is when ASM deletes keys from the source shard after migration.
    This is the most likely time for use-after-free bugs.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()

    # Create keys in specific slots that we'll migrate
    target_slot_keys = [slot_table[5], slot_table[10], slot_table[15]]
    
    env.debugPrint(f"Creating keys in target slots: {target_slot_keys}", force=True)
    
    with env.getClusterConnectionIfNeeded() as conn:
        for slot_key in target_slot_keys:
            for i in range(30):
                key = f"json:{{{slot_key}}}:{i}"
                # Large document with many string fields
                doc = {
                    "id": i,
                    "data": "Y" * 1000,  # Large string
                    "nested": {
                        f"field_{j}": f"Nested string {j} with data " + ("Z" * 200)
                        for j in range(20)
                    }
                }
                conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
    
    done = False
    errors = []
    connection_errors = []
    read_count = [0]
    
    def read_target_keys_aggressively():
        """Hammer the specific keys being migrated"""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            while not done:
                try:
                    slot_key = random.choice(target_slot_keys)
                    key_num = random.randint(0, 29)
                    key = f"json:{{{slot_key}}}:{key_num}"
                    
                    # Try multiple operations that access strings
                    thread_conn.execute_command("JSON.GET", key, "$.nested")
                    thread_conn.execute_command("JSON.GET", key, "$.data")
                    thread_conn.execute_command("JSON.GET", key, "$")
                    
                    read_count[0] += 3
                    
                except Exception as e:
                    error_str = str(e)
                    if "connection" in error_str.lower() or "broken" in error_str.lower() or "reset" in error_str.lower():
                        connection_errors.append(str(e))
                    elif "MOVED" not in error_str and "ASK" not in error_str:
                        errors.append(str(e))
    
    # Start threads hammering the keys
    threads = [threading.Thread(target=read_target_keys_aggressively) for _ in range(10)]
    for t in threads:
        t.start()
    
    time.sleep(0.2)
    
    env.debugPrint("Starting migration of target slots...", force=True)
    
    # Get slot ranges for our target keys
    first_conn = env.getConnection(0)
    
    try:
        # Migrate multiple times to increase crash probability
        for i in range(5):
            env.debugPrint(f"Migration cycle {i+1}/5", force=True)
            
            # Start migration
            migrate_slots_rapid(env)
            
            # Continue hammering during TRIM phase (very short window)
            time.sleep(0.1)
            
            # Check if shard is still alive
            try:
                first_conn.execute_command("PING")
            except Exception as e:
                connection_errors.append(f"Shard crash detected: {e}")
                break
    finally:
        done = True
        for t in threads:
            t.join(timeout=5)
    
    env.debugPrint(f"Total reads: {read_count[0]}", force=True)
    env.debugPrint(f"Connection errors: {len(connection_errors)}", force=True)
    
    if connection_errors:
        raise AssertionError(f"CONNECTION LOST - POSSIBLE CRASH! {len(connection_errors)} errors: {connection_errors[:5]}")
    
    if errors:
        env.debugPrint(f"Other errors: {len(errors)}", force=True)


# ============================================================================
# SHARED STRINGS CACHE TESTS - Tests from test_asm_shared_strings.py
# ============================================================================
def test_asm_shared_string_cache_race():
    """
    Extremely aggressive test targeting shared string cache race conditions.
    
    Strategy:
    1. Create MANY keys with the SAME strings (to fill cache)
    2. Perform rapid ASM migrations 
    3. Concurrently read/write these strings from multiple threads
    4. Force cache pressure to trigger eviction/insertion races
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()
    
    env.debugPrint("=== Starting Shared String Cache Race Test ===", force=True)
    
    # Configuration for maximum cache pressure
    num_keys = 200  # Many keys
    num_duplicate_strings = 20  # Same strings repeated across all keys
    operations_per_thread = 5000  # Many operations
    num_threads = 6  # Many concurrent threads
    
    # Create pool of duplicate strings that will be shared in cache
    SHARED_STRINGS = [
        f"SHARED_STRING_{i}_{'X' * 100}"  # Long strings to stress cache
        for i in range(num_duplicate_strings)
    ]
    
    slots = slot_table()
    
    # Create initial data with LOTS of duplicate strings
    env.debugPrint(f"Creating {num_keys} keys with {num_duplicate_strings} shared strings...", force=True)
    with env.getClusterConnectionIfNeeded() as conn:
        for i in range(num_keys):
            hslot = i * (len(slots) - 1) // (num_keys - 1)
            key = f"json:{{{slots[hslot]}}}"
            
            # Document with many references to the SAME strings
            doc = {
                f"field_{j}": SHARED_STRINGS[j % num_duplicate_strings]
                for j in range(50)  # 50 fields, all using shared strings
            }
            conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
    
    env.debugPrint("Initial data created. Starting stress test...", force=True)
    
    # Shared state
    done = False
    errors = []
    crashes_detected = [False]
    operation_counts = [0]
    cache_hits = [0]
    
    def migrate_rapidly():
        """Rapidly migrate slots back and forth."""
        try:
            while not done:
                # Get source and target nodes
                with env.getClusterConnectionIfNeeded() as conn:
                    nodes = conn.execute_command("CLUSTER", "NODES").split('\n')
                    node_ids = []
                    for node in nodes:
                        if 'master' in node and node.strip():
                            node_ids.append(node.split()[0])
                    
                    if len(node_ids) >= 2:
                        # Migrate first 5 slots back and forth rapidly
                        for slot in slots[:5]:
                            try:
                                # Migrate from node 0 to node 1
                                conn.execute_command(
                                    "CLUSTER", "MIGRATION", "IMPORT-START",
                                    str(slot), node_ids[0]
                                )
                                time.sleep(0.001)  # Tiny delay - maximum chance of collision
                                conn.execute_command(
                                    "CLUSTER", "MIGRATION", "IMPORT-FINISHED", str(slot)
                                )
                                
                                # Immediately migrate back
                                time.sleep(0.001)
                                conn.execute_command(
                                    "CLUSTER", "MIGRATION", "IMPORT-START",
                                    str(slot), node_ids[1]
                                )
                                time.sleep(0.001)
                                conn.execute_command(
                                    "CLUSTER", "MIGRATION", "IMPORT-FINISHED", str(slot)
                                )
                            except Exception as e:
                                if "CLUSTERDOWN" not in str(e):
                                    pass  # Ignore migration conflicts
        except Exception as e:
            errors.append(f"Migration thread error: {e}")
    
    def hammer_shared_strings():
        """Continuously modify strings that should be in cache."""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            ops = 0
            while ops < operations_per_thread and not done and not crashes_detected[0]:
                try:
                    key_idx = random.randint(0, num_keys - 1)
                    hslot = key_idx * (len(slots) - 1) // (num_keys - 1)
                    key = f"json:{{{slots[hslot]}}}"
                    field_idx = random.randint(0, 49)
                    
                    op_type = random.choice(["read", "write", "write"])
                    
                    if op_type == "read":
                        # Read a field (accesses shared string from cache)
                        result = thread_conn.execute_command("JSON.GET", key, f"$.field_{field_idx}")
                        if result:
                            doc = json.loads(result)
                            if doc and len(doc) > 0:
                                value = doc[0]
                                # Verify it's one of our shared strings
                                if value not in SHARED_STRINGS:
                                    errors.append(f"String corruption! Got: {value[:50]}, expected one of SHARED_STRINGS")
                                else:
                                    cache_hits[0] += 1
                    else:
                        # Write - replace with another shared string (cache insertion)
                        new_string = SHARED_STRINGS[random.randint(0, num_duplicate_strings - 1)]
                        thread_conn.execute_command("JSON.SET", key, f"$.field_{field_idx}", json.dumps(new_string))
                    
                    ops += 1
                    operation_counts[0] += 1
                    
                except Exception as e:
                    error_str = str(e)
                    if "MOVED" not in error_str and "ASK" not in error_str and "CLUSTERDOWN" not in error_str:
                        if "crash" in error_str.lower() or "segfault" in error_str.lower() or "connection" in error_str.lower():
                            crashes_detected[0] = True
                            errors.append(f"CRASH DETECTED: {e}")
                        else:
                            errors.append(f"Thread error: {e}")
    
    def read_entire_documents():
        """Read entire documents to access ALL shared strings at once."""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            ops = 0
            while ops < operations_per_thread // 2 and not done and not crashes_detected[0]:
                try:
                    key_idx = random.randint(0, num_keys - 1)
                    hslot = key_idx * (len(slots) - 1) // (num_keys - 1)
                    key = f"json:{{{slots[hslot]}}}"
                    
                    # Get entire document (accesses 50 shared strings)
                    result = thread_conn.execute_command("JSON.GET", key, "$")
                    if result:
                        doc = json.loads(result)
                        if doc and len(doc) > 0:
                            obj = doc[0]
                            # Access all string values (forces cache lookups)
                            for field_key, value in obj.items():
                                if value not in SHARED_STRINGS:
                                    errors.append(f"Corruption in {key}.{field_key}: {value[:50]}")
                                    
                    ops += 1
                    operation_counts[0] += 1
                    
                except Exception as e:
                    error_str = str(e)
                    if "MOVED" not in error_str and "ASK" not in error_str and "CLUSTERDOWN" not in error_str:
                        if "crash" in error_str.lower() or "segfault" in error_str.lower():
                            crashes_detected[0] = True
                            errors.append(f"CRASH: {e}")
    
    # Start migration thread
    migration_thread = threading.Thread(target=migrate_rapidly)
    migration_thread.start()
    
    # Give migration a moment to start
    time.sleep(0.2)
    
    # Start worker threads
    threads = []
    for _ in range(num_threads // 2):
        threads.append(threading.Thread(target=hammer_shared_strings))
        threads.append(threading.Thread(target=read_entire_documents))
    
    env.debugPrint(f"Starting {len(threads)} worker threads...", force=True)
    for t in threads:
        t.start()
    
    # Let it run for a while
    time.sleep(8)
    
    # Stop everything
    done = True
    env.debugPrint("Stopping threads...", force=True)
    
    for t in threads:
        t.join(timeout=5)
    migration_thread.join(timeout=5)
    
    env.debugPrint(f"Total operations: {operation_counts[0]}", force=True)
    env.debugPrint(f"Cache hits verified: {cache_hits[0]}", force=True)
    env.debugPrint(f"Errors: {len(errors)}", force=True)
    
    if crashes_detected[0]:
        env.assertTrue(False, "CRASH DETECTED during shared string cache stress test!")
    
    if errors:
        unique_errors = list(set(errors))[:20]
        env.debugPrint(f"Unique errors: {unique_errors}", force=True)
        env.assertTrue(False, f"Detected {len(errors)} errors during shared string cache stress: {unique_errors[:5]}")
    
    env.debugPrint("=== Shared String Cache Race Test PASSED ===", force=True)


def test_asm_string_cache_eviction_race():
    """
    Test specifically targeting cache eviction race conditions.
    
    Create MORE strings than can fit in cache, forcing eviction,
    while doing rapid ASM migrations.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()
    
    env.debugPrint("=== Starting Cache Eviction Race Test ===", force=True)
    
    # Create LOTS of unique strings to force cache eviction
    num_keys = 100
    strings_per_key = 100  # Will exceed typical cache size
    
    slots = slot_table()
    
    env.debugPrint(f"Creating {num_keys * strings_per_key} unique strings to force cache eviction...", force=True)
    
    errors = []
    done = False
    
    # Create keys with many unique strings
    with env.getClusterConnectionIfNeeded() as conn:
        for i in range(num_keys):
            hslot = i * (len(slots) - 1) // (num_keys - 1)
            key = f"json:{{{slots[hslot]}}}"
            
            doc = {
                f"str_{j}": f"UNIQUE_STRING_{i}_{j}_{'Y' * 50}"
                for j in range(strings_per_key)
            }
            conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
    
    def rapid_eviction_operations():
        """Rapidly read different strings to cause cache eviction."""
        with env.getClusterConnectionIfNeeded() as thread_conn:
            for _ in range(2000):
                try:
                    key_idx = random.randint(0, num_keys - 1)
                    hslot = key_idx * (len(slots) - 1) // (num_keys - 1)
                    key = f"json:{{{slots[hslot]}}}"
                    field_idx = random.randint(0, strings_per_key - 1)
                    
                    # This will cause cache misses and insertions/evictions
                    thread_conn.execute_command("JSON.GET", key, f"$.str_{field_idx}")
                    
                except Exception as e:
                    if "MOVED" not in str(e) and "ASK" not in str(e):
                        errors.append(str(e))
    
    # Start threads
    threads = [threading.Thread(target=rapid_eviction_operations) for _ in range(4)]
    
    # Also do migrations
    def do_migration():
        time.sleep(0.5)
        migrate_slots_back_and_forth(env)
    
    migration_thread = threading.Thread(target=do_migration)
    migration_thread.start()
    
    for t in threads:
        t.start()
    
    for t in threads:
        t.join()
    
    migration_thread.join()
    
    env.debugPrint(f"Cache eviction test completed with {len(errors)} errors", force=True)
    
    if errors:
        env.debugPrint(f"Sample errors: {errors[:5]}", force=True)
        env.assertTrue(False, f"Detected {len(errors)} errors during cache eviction test")


def migrate_slots_back_and_forth(env):
    """Migrate slots between shards."""
    with env.getClusterConnectionIfNeeded() as conn:
        try:
            nodes = conn.execute_command("CLUSTER", "NODES").split('\n')
            node_ids = []
            for node in nodes:
                if 'master' in node and node.strip():
                    node_ids.append(node.split()[0])
            
            if len(node_ids) >= 2:
                slots = slot_table()
                for slot in slots[:10]:
                    # Forward
                    conn.execute_command("CLUSTER", "MIGRATION", "IMPORT-START", str(slot), node_ids[0])
                    time.sleep(0.05)
                    conn.execute_command("CLUSTER", "MIGRATION", "IMPORT-FINISHED", str(slot))
                    
                    # Back
                    conn.execute_command("CLUSTER", "MIGRATION", "IMPORT-START", str(slot), node_ids[1])
                    time.sleep(0.05)
                    conn.execute_command("CLUSTER", "MIGRATION", "IMPORT-FINISHED", str(slot))
        except Exception as e:
            if "CLUSTERDOWN" not in str(e):
                pass  # OK



# ==============================================================================
# THREAD-SAFE CACHE TESTS - CLUSTER MODE
# These tests verify the Mutex<HashSet> cache prevents race conditions
# ==============================================================================

def test_shared_strings_concurrent_writes_cluster():
    """
    Test: Concurrent writes with shared field names in cluster with ASM.
    
    This test reproduces the race condition that occurs when:
    - Multiple threads write JSON documents with shared field names
    - ASM migration is happening in background
    - String cache is accessed from both main and background threads
    
    Without thread-safe cache: This would cause data races
    With thread-safe cache: All operations are synchronized
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()
    
    # Complex JSON with many shared field names (these get cached)
    template_json = {
        "user_id": 0,
        "username": "user",
        "email": "user@example.com",
        "profile": {
            "first_name": "First",
            "last_name": "Last",
            "age": 30,
            "country": "USA",
            "city": "New York",
            "address": "123 Main St",
            "postal_code": "10001",
            "phone": "555-1234"
        },
        "settings": {
            "notification_email": True,
            "notification_sms": False,
            "newsletter_subscribed": True,
            "theme": "dark",
            "language": "en",
            "timezone": "UTC"
        },
        "metadata": {
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z",
            "last_login": "2025-01-01T00:00:00Z",
            "login_count": 0,
            "is_active": True,
            "is_verified": True
        }
    }
    
    conn = env.getConnection()
    errors = []
    success_count = [0]
    
    def write_worker(worker_id, iterations):
        """Worker thread that writes JSON documents"""
        try:
            thread_conn = env.getConnection()
            for i in range(iterations):
                doc = template_json.copy()
                doc["user_id"] = worker_id * 1000 + i
                doc["username"] = f"user_{worker_id}_{i}"
                
                key = f"user:{worker_id}:{i}"
                thread_conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
                
                # Verify immediately
                result = json.loads(thread_conn.execute_command("JSON.GET", key, "$"))
                if result[0]["user_id"] != doc["user_id"]:
                    errors.append(f"Data corruption in worker {worker_id} iteration {i}")
                else:
                    success_count[0] += 1
                    
        except Exception as e:
            errors.append(f"Worker {worker_id} error: {str(e)}")
    
    # Start concurrent writers
    threads = []
    num_workers = 5
    iterations_per_worker = 20
    
    print(f"Starting {num_workers} concurrent workers, {iterations_per_worker} iterations each")
    
    for worker_id in range(num_workers):
        t = threading.Thread(target=write_worker, args=(worker_id, iterations_per_worker))
        t.start()
        threads.append(t)
    
    # While writes are happening, trigger migration
    time.sleep(0.5)
    
    # Get cluster info and migrate slots
    nodes_info = conn.execute_command("CLUSTER", "NODES").split("\n")
    nodes = [line for line in nodes_info if line and "master" in line]
    
    if len(nodes) >= 2:
        node1_id = nodes[0].split()[0]
        node2_id = nodes[1].split()[0]
        
        # Migrate some slots during writes
        print("Migrating slots during concurrent writes...")
        try:
            conn.execute_command("CLUSTER", "SETSLOT", "100", "MIGRATING", node2_id)
            conn.execute_command("CLUSTER", "SETSLOT", "100", "IMPORTING", node1_id)
            conn.execute_command("CLUSTER", "SETSLOT", "100", "NODE", node2_id)
        except Exception as e:
            print(f"Migration command error (expected in some cases): {e}")
    
    # Wait for all workers
    for t in threads:
        t.join(timeout=30)
    
    if errors:
        print(f"❌ Errors detected: {errors}")
        env.assertTrue(False, f"Race condition detected: {errors[0]}")
    else:
        print(f"✅ All {success_count[0]} operations completed successfully")
        env.assertTrue(success_count[0] > 0)


def test_rapid_json_updates_shared_strings():
    """
    Test: Rapid updates to same keys with shared strings during cluster operations.
    
    Stresses the string cache with many threads updating the same keys,
    maximizing contention and exposing race conditions.
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()
    
    conn = env.getConnection()
    
    # Seed initial data
    for i in range(10):
        key = f"shared:key:{i}"
        doc = {
            "counter": 0,
            "field_a": "value",
            "field_b": "value", 
            "field_c": "value",
            "nested": {
                "field_x": "value",
                "field_y": "value",
                "field_z": "value"
            }
        }
        conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
    
    errors = []
    
    def update_worker(worker_id, iterations):
        try:
            thread_conn = env.getConnection()
            for i in range(iterations):
                key = f"shared:key:{i % 10}"
                thread_conn.execute_command("JSON.NUMINCRBY", key, "$.counter", 1)
                
                result = thread_conn.execute_command("JSON.GET", key, "$")
                if not result or "counter" not in result:
                    errors.append(f"Worker {worker_id}: Structure corrupted at iteration {i}")
                    return
                    
        except Exception as e:
            errors.append(f"Worker {worker_id} error: {str(e)}")
    
    threads = []
    num_workers = 10
    iterations = 50
    
    print(f"Starting {num_workers} workers doing {iterations} rapid updates each")
    
    for worker_id in range(num_workers):
        t = threading.Thread(target=update_worker, args=(worker_id, iterations))
        t.start()
        threads.append(t)
    
    for t in threads:
        t.join(timeout=30)
    
    if errors:
        print(f"❌ Race condition errors: {errors}")
        env.assertTrue(False, f"Race condition detected: {errors[0]}")
    else:
        total_updates = 0
        for i in range(10):
            key = f"shared:key:{i}"
            result = json.loads(conn.execute_command("JSON.GET", key, "$.counter"))
            total_updates += result[0]
        
        expected = num_workers * iterations
        print(f"✅ All updates completed. Total: {total_updates} (expected: {expected})")
        env.assertTrue(total_updates == expected)


def test_string_cache_thread_safety_stress():
    """
    Stress test: Maximum contention on string cache with diverse operations.
    
    Hammers the string cache from many threads with:
    - New documents (new strings)
    - Updates (reusing cached strings)
    - Deletes (cleanup)
    - Reads (accessing strings)
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()
    
    conn = env.getConnection()
    errors = []
    operation_counts = {"set": [0], "get": [0], "del": [0], "update": [0]}
    
    common_fields = [
        "id", "name", "email", "status", "created_at", "updated_at",
        "field_a", "field_b", "field_c", "field_d", "field_e",
        "nested_x", "nested_y", "nested_z"
    ]
    
    def stress_worker(worker_id, iterations):
        try:
            thread_conn = env.getConnection()
            for i in range(iterations):
                op = random.choice(["set", "get", "update", "del"])
                key = f"stress:{worker_id}:{i % 20}"
                
                try:
                    if op == "set":
                        doc = {field: f"value_{worker_id}_{i}" for field in common_fields}
                        thread_conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
                        operation_counts["set"][0] += 1
                    elif op == "get":
                        thread_conn.execute_command("JSON.GET", key, "$")
                        operation_counts["get"][0] += 1
                    elif op == "update":
                        thread_conn.execute_command("JSON.SET", key, "$.status", '"updated"')
                        operation_counts["update"][0] += 1
                    elif op == "del":
                        thread_conn.execute_command("JSON.DEL", key)
                        operation_counts["del"][0] += 1
                except Exception as e:
                    if "not exist" not in str(e).lower():
                        errors.append(f"Worker {worker_id} op={op}: {str(e)}")
        except Exception as e:
            errors.append(f"Worker {worker_id} fatal error: {str(e)}")
    
    threads = []
    num_workers = 8
    iterations = 100
    
    print(f"Starting {num_workers} stress workers doing {iterations} mixed operations each")
    
    for worker_id in range(num_workers):
        t = threading.Thread(target=stress_worker, args=(worker_id, iterations))
        t.start()
        threads.append(t)
    
    for t in threads:
        t.join(timeout=60)
    
    if errors:
        print(f"❌ Race conditions: {errors[:5]}")
        env.assertTrue(False, f"String cache race condition: {errors[0]}")
    else:
        print(f"✅ Stress test passed!")
        print(f"   Operations: SET={operation_counts['set'][0]}, "
              f"GET={operation_counts['get'][0]}, "
              f"UPDATE={operation_counts['update'][0]}, "
              f"DEL={operation_counts['del'][0]}")
        env.assertTrue(sum(v[0] for v in operation_counts.values()) > 0)
