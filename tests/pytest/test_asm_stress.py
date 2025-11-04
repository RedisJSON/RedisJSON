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

