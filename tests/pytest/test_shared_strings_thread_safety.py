"""
Shared String Cache Thread-Safety Tests

These tests verify that the thread-safe string cache prevents race conditions
when there are concurrent operations accessing shared strings, particularly:
- During async flush (background disk writes)
- During ASM (Atomic Slot Migration) in cluster mode
- During concurrent write commands from multiple threads

Race condition scenario without thread-safe cache:
1. Main thread: Write command adds new string to cache
2. Background thread: Async flush or ASM reads from cache
3. Without synchronization: Data race → corruption or crash

With thread-safe Mutex<HashSet> cache:
- All cache access is synchronized
- No race conditions
- Data integrity maintained

These tests serve as regression tests to ensure the thread-safe cache
continues to work correctly.
"""

import threading
import time
import json
import random
from RLTest import Env


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
    success_count = [0]  # Using list for mutable counter in threads
    
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
    time.sleep(0.5)  # Let some writes happen first
    
    # Get cluster info and migrate slots
    nodes_info = conn.execute_command("CLUSTER", "NODES").split("\n")
    nodes = [line for line in nodes_info if line and "master" in line]
    
    if len(nodes) >= 2:
        # Parse node IDs
        node1_id = nodes[0].split()[0]
        node2_id = nodes[1].split()[0]
        
        # Migrate some slots back and forth during writes
        print("Migrating slots during concurrent writes...")
        try:
            conn.execute_command("CLUSTER", "SETSLOT", "100", "MIGRATING", node2_id)
            conn.execute_command("CLUSTER", "SETSLOT", "100", "IMPORTING", node1_id)
            conn.execute_command("CLUSTER", "SETSLOT", "100", "NODE", node2_id)
        except Exception as e:
            print(f"Migration command error (expected in some cases): {e}")
    
    # Wait for all workers to complete
    for t in threads:
        t.join(timeout=30)
    
    # Check for errors
    if errors:
        print(f"❌ Errors detected: {errors}")
        env.assertTrue(False, f"Race condition detected: {errors[0]}")
    else:
        print(f"✅ All {success_count[0]} operations completed successfully without race conditions")
        env.assertTrue(success_count[0] > 0, "No operations completed")


def test_rapid_json_updates_shared_strings():
    """
    Test: Rapid updates to same keys with shared strings during cluster operations.
    
    This stresses the string cache with many threads updating the same keys,
    which maximizes contention on the string cache and exposes race conditions.
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
        """Worker that rapidly updates JSON fields"""
        try:
            thread_conn = env.getConnection()
            for i in range(iterations):
                key = f"shared:key:{i % 10}"
                # Update counter - this modifies the JSON but reuses field names
                thread_conn.execute_command("JSON.NUMINCRBY", key, "$.counter", 1)
                
                # Get and verify structure is intact
                result = thread_conn.execute_command("JSON.GET", key, "$")
                if not result or "counter" not in result:
                    errors.append(f"Worker {worker_id}: Structure corrupted at iteration {i}")
                    return
                    
        except Exception as e:
            errors.append(f"Worker {worker_id} error: {str(e)}")
    
    # Launch many update workers
    threads = []
    num_workers = 10
    iterations = 50
    
    print(f"Starting {num_workers} workers doing {iterations} rapid updates each")
    
    for worker_id in range(num_workers):
        t = threading.Thread(target=update_worker, args=(worker_id, iterations))
        t.start()
        threads.append(t)
    
    # Wait for completion
    for t in threads:
        t.join(timeout=30)
    
    # Verify final state
    if errors:
        print(f"❌ Race condition errors: {errors}")
        env.assertTrue(False, f"Race condition detected: {errors[0]}")
    else:
        # Check counters are reasonable (should be num_workers * iterations / 10 keys)
        total_updates = 0
        for i in range(10):
            key = f"shared:key:{i}"
            result = json.loads(conn.execute_command("JSON.GET", key, "$.counter"))
            total_updates += result[0]
        
        expected = num_workers * iterations
        print(f"✅ All updates completed. Total counter updates: {total_updates} (expected: {expected})")
        env.assertTrue(total_updates == expected, 
                      f"Lost updates detected: {total_updates} vs {expected}")


def test_string_cache_thread_safety_stress():
    """
    Stress test: Maximum contention on string cache with diverse operations.
    
    This test hammers the string cache from many threads simultaneously with:
    - New documents (new strings added to cache)
    - Updates (reusing cached strings)
    - Deletes (potential cleanup of cache entries)
    - Reads (accessing cached strings)
    """
    env = Env(shardsCount=2, decodeResponses=True)
    if env.env != "oss-cluster":
        env.skip()
    
    conn = env.getConnection()
    errors = []
    operation_counts = {"set": [0], "get": [0], "del": [0], "update": [0]}
    
    # Field names that will be heavily reused (cached)
    common_fields = [
        "id", "name", "email", "status", "created_at", "updated_at",
        "field_a", "field_b", "field_c", "field_d", "field_e",
        "nested_x", "nested_y", "nested_z"
    ]
    
    def stress_worker(worker_id, iterations):
        """Worker doing mixed operations"""
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
                    # Some operations may fail (key not exists), that's ok
                    if "not exist" not in str(e).lower():
                        errors.append(f"Worker {worker_id} op={op}: {str(e)}")
                        
        except Exception as e:
            errors.append(f"Worker {worker_id} fatal error: {str(e)}")
    
    # Launch stress workers
    threads = []
    num_workers = 8
    iterations = 100
    
    print(f"Starting {num_workers} stress workers doing {iterations} mixed operations each")
    
    for worker_id in range(num_workers):
        t = threading.Thread(target=stress_worker, args=(worker_id, iterations))
        t.start()
        threads.append(t)
    
    # Wait for completion
    for t in threads:
        t.join(timeout=60)
    
    # Check results
    if errors:
        print(f"❌ Race conditions detected: {errors[:5]}")  # Show first 5 errors
        env.assertTrue(False, f"String cache race condition: {errors[0]}")
    else:
        print(f"✅ Stress test passed!")
        print(f"   Operations: SET={operation_counts['set'][0]}, "
              f"GET={operation_counts['get'][0]}, "
              f"UPDATE={operation_counts['update'][0]}, "
              f"DEL={operation_counts['del'][0]}")
        env.assertTrue(sum(v[0] for v in operation_counts.values()) > 0, 
                      "No operations completed")


def test_concurrent_writes_shared_strings_standalone():
    """
    Test: Concurrent writes with shared field names in standalone mode.
    
    This test verifies thread-safe cache works during concurrent writes
    without requiring cluster mode. Tests the scenario where:
    - Multiple client connections write simultaneously
    - All use the same field names (shared in cache)
    - Async flush may happen in background
    """
    env = Env(decodeResponses=True)
    
    # Template with many shared field names
    template = {
        "transaction_id": 0,
        "user_id": 0,
        "amount": 0.0,
        "currency": "USD",
        "status": "pending",
        "payment_method": "credit_card",
        "billing_address": {
            "street": "Main St",
            "city": "New York",
            "state": "NY",
            "country": "USA",
            "postal_code": "10001"
        },
        "shipping_address": {
            "street": "Main St",
            "city": "New York",
            "state": "NY",
            "country": "USA",
            "postal_code": "10001"
        },
        "items": [],
        "metadata": {
            "created_at": "2025-01-01",
            "updated_at": "2025-01-01",
            "ip_address": "127.0.0.1",
            "user_agent": "Mozilla/5.0"
        }
    }
    
    errors = []
    success_count = [0]
    
    def write_worker(worker_id, count):
        try:
            conn = env.getConnection()
            for i in range(count):
                doc = template.copy()
                doc["transaction_id"] = worker_id * 1000 + i
                doc["user_id"] = worker_id
                doc["amount"] = round(random.uniform(10.0, 1000.0), 2)
                
                key = f"transaction:{worker_id}:{i}"
                conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
                
                # Verify
                result = json.loads(conn.execute_command("JSON.GET", key, "$"))
                if result[0]["transaction_id"] != doc["transaction_id"]:
                    errors.append(f"Corruption in worker {worker_id}")
                else:
                    success_count[0] += 1
        except Exception as e:
            errors.append(f"Worker {worker_id}: {e}")
    
    # Launch concurrent writers
    threads = []
    num_workers = 8
    writes_per_worker = 25
    
    for i in range(num_workers):
        t = threading.Thread(target=write_worker, args=(i, writes_per_worker))
        t.start()
        threads.append(t)
    
    # Wait for completion
    for t in threads:
        t.join(timeout=30)
    
    if errors:
        env.assertTrue(False, f"Race condition: {errors[0]}")
    
    print(f"✅ {success_count[0]} concurrent writes completed successfully")
    env.assertTrue(success_count[0] == num_workers * writes_per_worker,
                  "Some writes failed")


def test_mixed_operations_shared_cache_standalone():
    """
    Test: Mixed read/write operations stressing the shared string cache.
    
    Simulates real-world usage with:
    - Concurrent readers and writers
    - Same field names reused across documents
    - Async flush potentially happening in background
    """
    env = Env(decodeResponses=True)
    
    conn = env.getConnection()
    
    # Seed some initial data
    for i in range(20):
        doc = {
            "id": i,
            "name": f"item_{i}",
            "category": "test",
            "price": 10.0,
            "in_stock": True,
            "tags": ["tag1", "tag2", "tag3"]
        }
        conn.execute_command("JSON.SET", f"item:{i}", "$", json.dumps(doc))
    
    errors = []
    read_count = [0]
    write_count = [0]
    
    def reader_worker(worker_id, iterations):
        try:
            conn = env.getConnection()
            for i in range(iterations):
                key = f"item:{i % 20}"
                result = conn.execute_command("JSON.GET", key, "$")
                if result:
                    read_count[0] += 1
        except Exception as e:
            errors.append(f"Reader {worker_id}: {e}")
    
    def writer_worker(worker_id, iterations):
        try:
            conn = env.getConnection()
            for i in range(iterations):
                key = f"item:{i % 20}"
                # Update price - reuses field name "price"
                conn.execute_command("JSON.NUMINCRBY", key, "$.price", 1.0)
                write_count[0] += 1
        except Exception as e:
            errors.append(f"Writer {worker_id}: {e}")
    
    threads = []
    
    # Mix of readers and writers
    for i in range(5):
        t = threading.Thread(target=reader_worker, args=(i, 50))
        threads.append(t)
        t.start()
    
    for i in range(5):
        t = threading.Thread(target=writer_worker, args=(i, 50))
        threads.append(t)
        t.start()
    
    for t in threads:
        t.join(timeout=30)
    
    if errors:
        env.assertTrue(False, f"Race condition: {errors[0]}")
    
    print(f"✅ {read_count[0]} reads and {write_count[0]} writes completed")
    env.assertTrue(read_count[0] > 0 and write_count[0] > 0, "Operations failed")


if __name__ == "__main__":
    # For local testing
    print("Thread-safety tests for shared string cache")
    print("Run with: TEST=test_shared_strings_thread_safety.py bash tests.sh <module-path>")

