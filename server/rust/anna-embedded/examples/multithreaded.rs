//! Multi-threaded example: concurrent reads and writes from multiple threads.

use anna_embedded::EmbeddedKvs;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

fn main() {
    env_logger::init();

    let num_actors = 4;
    let num_writer_threads = 4;
    let num_reader_threads = 4;
    let keys_per_writer = 1000;

    let kvs = Arc::new(EmbeddedKvs::new(num_actors).expect("failed to create embedded KVS"));
    println!(
        "Created embedded KVS with {} actors, launching {} writers and {} readers",
        kvs.num_actors(),
        num_writer_threads,
        num_reader_threads
    );

    let start = Instant::now();

    // Spawn writer threads.
    let mut handles = Vec::new();
    for writer_id in 0..num_writer_threads {
        let kvs = Arc::clone(&kvs);
        handles.push(thread::spawn(move || {
            for i in 0..keys_per_writer {
                let key = format!("w{}_key_{}", writer_id, i);
                let value = format!("value_{}_{}", writer_id, i);
                kvs.put(&key, value.as_bytes()).unwrap();
            }
        }));
    }

    // Wait for writers to finish.
    for h in handles {
        h.join().unwrap();
    }
    let write_elapsed = start.elapsed();
    let total_writes = num_writer_threads * keys_per_writer;
    println!(
        "Wrote {} keys in {:.2?} ({:.0} ops/sec)",
        total_writes,
        write_elapsed,
        total_writes as f64 / write_elapsed.as_secs_f64()
    );

    // Spawn reader threads that read all keys.
    let read_start = Instant::now();
    let mut handles = Vec::new();
    for reader_id in 0..num_reader_threads {
        let kvs = Arc::clone(&kvs);
        handles.push(thread::spawn(move || {
            let mut found = 0u64;
            let mut missing = 0u64;
            // Each reader reads keys from all writers.
            for writer_id in 0..num_writer_threads {
                for i in 0..keys_per_writer {
                    let key = format!("w{}_key_{}", writer_id, i);
                    match kvs.get(&key).unwrap() {
                        Some(_) => found += 1,
                        None => missing += 1,
                    }
                }
            }
            println!("Reader {}: found={}, missing={}", reader_id, found, missing);
            found
        }));
    }

    let mut total_reads = 0u64;
    for h in handles {
        total_reads += h.join().unwrap();
    }
    let read_elapsed = read_start.elapsed();
    println!(
        "Read {} keys in {:.2?} ({:.0} ops/sec)",
        total_reads,
        read_elapsed,
        total_reads as f64 / read_elapsed.as_secs_f64()
    );

    // Final scan.
    let entries = kvs.scan("").unwrap();
    println!("Total keys in store: {}", entries.len());

    println!("Done.");
}
