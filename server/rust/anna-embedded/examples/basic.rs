//! Basic example: single-threaded put/get/delete/scan.

use anna_embedded::EmbeddedKvs;

fn main() {
    env_logger::init();

    let kvs = EmbeddedKvs::new(2).expect("failed to create embedded KVS");
    println!("Created embedded KVS with {} actors", kvs.num_actors());

    // Put some values.
    kvs.put("greeting", b"hello world").unwrap();
    kvs.put("name", b"Anna").unwrap();
    kvs.put("version", b"0.1.0").unwrap();

    // Get a value.
    let value = kvs.get("greeting").unwrap();
    println!(
        "greeting = {:?}",
        value.map(|v| String::from_utf8_lossy(&v).to_string())
    );

    // Scan all keys.
    let entries = kvs.scan("").unwrap();
    println!("All keys ({}):", entries.len());
    for entry in &entries {
        println!("  {} ({} bytes)", entry.key, entry.size);
    }

    // Delete a key.
    kvs.delete("name").unwrap();
    let deleted = kvs.get("name").unwrap();
    println!("name after delete = {:?}", deleted);

    // Scan with prefix.
    kvs.put("user:alice", b"alice@example.com").unwrap();
    kvs.put("user:bob", b"bob@example.com").unwrap();
    kvs.put("config:timeout", b"30").unwrap();

    let users = kvs.scan("user:").unwrap();
    println!("Keys with prefix 'user:' ({}):", users.len());
    for entry in &users {
        println!("  {}", entry.key);
    }

    println!("Done.");
}
