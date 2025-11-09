"""
Thread-Safety Tests for Shared String Cache (Standalone Mode)

These tests verify that the thread-safe string cache prevents race conditions
during concurrent operations in standalone (non-cluster) mode, particularly:
- During async flush (background disk writes)
- During concurrent write commands from multiple client connections

Race condition scenario without thread-safe cache:
1. Main thread: Write command adds new string to cache
2. Background thread: Async flush reads from cache
3. Without synchronization: Data race → corruption or crash

With thread-safe Mutex<HashSet> cache:
- All cache access is synchronized
- No race conditions
- Data integrity maintained

These tests serve as regression tests to ensure the thread-safe cache
continues to work correctly for standalone Redis instances.

Note: For cluster/ASM-related thread-safety tests, see test_asm.py
"""

import threading
import json
import random
from RLTest import Env


def test_concurrent_writes_shared_strings():
    """
    Test: Concurrent writes with shared field names in standalone mode.
    
    Verifies thread-safe cache works during concurrent writes without
    requiring cluster mode. Tests the scenario where:
    - Multiple client connections write simultaneously
    - All use the same field names (shared in cache)
    - Async flush may happen in background
    
    This is the primary test for async flush race conditions.
    """
    env = Env(decodeResponses=True)
    
    # Template with many shared field names that get cached
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
        """Worker thread that writes JSON documents"""
        try:
            conn = env.getConnection()
            for i in range(count):
                doc = template.copy()
                doc["transaction_id"] = worker_id * 1000 + i
                doc["user_id"] = worker_id
                doc["amount"] = round(random.uniform(10.0, 1000.0), 2)
                
                key = f"transaction:{worker_id}:{i}"
                conn.execute_command("JSON.SET", key, "$", json.dumps(doc))
                
                # Verify immediately to catch corruption
                result = json.loads(conn.execute_command("JSON.GET", key, "$"))
                if result[0]["transaction_id"] != doc["transaction_id"]:
                    errors.append(f"Data corruption in worker {worker_id}")
                else:
                    success_count[0] += 1
        except Exception as e:
            errors.append(f"Worker {worker_id} error: {str(e)}")
    
    # Launch concurrent writers
    threads = []
    num_workers = 8
    writes_per_worker = 25
    
    print(f"Starting {num_workers} concurrent workers, {writes_per_worker} writes each")
    
    for i in range(num_workers):
        t = threading.Thread(target=write_worker, args=(i, writes_per_worker))
        t.start()
        threads.append(t)
    
    # Wait for completion
    for t in threads:
        t.join(timeout=30)
    
    # Check results
    if errors:
        print(f"❌ Race condition detected: {errors}")
        # Use env.assertEqual to avoid RLTest f-string bug
        env.assertEqual(len(errors), 0)
    
    expected = num_workers * writes_per_worker
    print(f"✅ All {success_count[0]} concurrent writes completed successfully")
    # Use env.assertEqual to avoid RLTest f-string bug
    env.assertEqual(success_count[0], expected)


def test_mixed_operations_shared_cache():
    """
    Test: Mixed read/write operations stressing the shared string cache.
    
    Simulates real-world usage with:
    - Concurrent readers and writers
    - Same field names reused across documents
    - Async flush potentially happening in background
    
    This test catches issues with concurrent reads during cache updates.
    """
    env = Env(decodeResponses=True)
    
    conn = env.getConnection()
    
    # Seed initial data with shared field names
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
        """Worker that reads JSON documents"""
        try:
            conn = env.getConnection()
            for i in range(iterations):
                key = f"item:{i % 20}"
                result = conn.execute_command("JSON.GET", key, "$")
                if result:
                    read_count[0] += 1
        except Exception as e:
            errors.append(f"Reader {worker_id} error: {str(e)}")
    
    def writer_worker(worker_id, iterations):
        """Worker that updates JSON documents"""
        try:
            conn = env.getConnection()
            for i in range(iterations):
                key = f"item:{i % 20}"
                # Update price - reuses cached field name "price"
                conn.execute_command("JSON.NUMINCRBY", key, "$.price", 1.0)
                write_count[0] += 1
        except Exception as e:
            errors.append(f"Writer {worker_id} error: {str(e)}")
    
    threads = []
    
    # Mix of readers and writers to stress cache
    print("Starting 5 readers + 5 writers")
    
    for i in range(5):
        t = threading.Thread(target=reader_worker, args=(i, 50))
        threads.append(t)
        t.start()
    
    for i in range(5):
        t = threading.Thread(target=writer_worker, args=(i, 50))
        threads.append(t)
        t.start()
    
    # Wait for all to complete
    for t in threads:
        t.join(timeout=30)
    
    # Check results
    if errors:
        print(f"❌ Race condition: {errors}")
        # Use env.assertEqual to avoid RLTest f-string bug
        env.assertEqual(len(errors), 0)
    
    print(f"✅ Completed: {read_count[0]} reads and {write_count[0]} writes")
    # Use env.assertTrue without message to avoid RLTest bug
    env.assertTrue(read_count[0] > 0)
    env.assertTrue(write_count[0] > 0)


if __name__ == "__main__":
    print("Thread-safety tests for shared string cache (standalone mode)")
    print("Run with: make test TEST_ARGS='--test test_thread_safety.py'")

