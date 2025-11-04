"""
ASM tests specifically targeting shared string cache race conditions.

These tests aim to expose thread-safety issues in the shared string cache
by creating maximum pressure with duplicate strings during slot migrations.
"""

import time
import json
import random
import threading
from RLTest import Env
from includes import *

def slot_table():
    """Compute slot table for distributing keys across slots."""
    def crc16(key):
        crc = 0xFFFF
        for byte in key.encode('utf-8'):
            crc ^= byte
            for _ in range(8):
                if crc & 1:
                    crc = (crc >> 1) ^ 0xA001
                else:
                    crc >>= 1
        return crc & 0xFFFF
    
    # Generate keys that map to different slots
    slots = []
    for i in range(100):
        key = f"test{i}"
        tag = f"{{{key}}}"
        slot = crc16(tag)
        if slot not in slots:
            slots.append(slot)
        if len(slots) >= 50:
            break
    return sorted(slots)


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

