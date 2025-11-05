"""
Stress tests for ASM (Atomic Slots Migration) in RedisJSON.

These tests are specifically designed to expose race conditions in shared string cache
during concurrent operations and slot migrations.
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


# Smaller slot table for focused testing on specific slots
slot_table = [
    "06S", "Qi", "5L5", "4Iu", "4gY", "460", "1Y7", "1LV", "0QG", "ru",
    "7Ok", "4ji", "4DE", "65n", "2JH", "I8", "F9", "SX", "7nF", "4KD",
]


@dataclass(frozen=True)
class SlotRange:
    """Represents a range of hash slots in the cluster."""
    start: int
    end: int

    @staticmethod
    def from_str(s: str):
        start, end = map(int, s.split("-"))
        assert 0 <= start <= end < 2**14
        return SlotRange(start, end)


@dataclass
class ClusterNode:
    """Represents a node in the Redis cluster."""
    id: str
    ip: str
    port: int
    cport: int
    hostname: Optional[str]
    flags: Set[str]
    master: str
    ping_sent: int
    pong_recv: int
    config_epoch: int
    link_state: bool
    slots: Set[SlotRange]

    @staticmethod
    def from_str(s: str):
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
                    result = thread_conn.execute_command("JSON.GET", key)
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

