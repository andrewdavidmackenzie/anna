//! Consistent hash ring with C FFI for Anna KVS.
//!
//! This crate provides a single canonical hash ring implementation shared
//! across all Anna components (Rust server, C++ server, and all client
//! libraries). The C API allows linking from C/C++, Python (ctypes), and
//! Go (cgo).

use anna_server_common::hash_ring::ConsistentHashRing;
use anna_server_common::threads::MAX_TID;
use std::ffi::CStr;
use std::os::raw::c_char;

/// Opaque handle to a consistent hash ring.
///
/// cbindgen:ignore
pub struct AnnaHashRing {
    ring: ConsistentHashRing,
    global: bool,
    base_offset: u32,
}

/// Result of a server lookup: public IP, private IP, thread ID.
#[repr(C)]
#[derive(Clone)]
pub struct ServerInfo {
    pub public_ip: *mut c_char,
    pub private_ip: *mut c_char,
    pub tid: u32,
}

// ── Lifecycle ───────────────────────────────────────────────────────

/// Create a new hash ring.
///
/// `global`: if true, uses the global hasher (for cross-node key distribution).
///           if false, uses the local hasher (for intra-node thread distribution).
/// `base_offset`: port base offset for address generation.
#[no_mangle]
pub extern "C" fn anna_hashring_new(global: bool, base_offset: u32) -> *mut AnnaHashRing {
    Box::into_raw(Box::new(AnnaHashRing {
        ring: ConsistentHashRing::new(),
        global,
        base_offset,
    }))
}

/// Free a hash ring.
///
/// # Safety
/// `ring` must be a valid pointer returned by `anna_hashring_new`.
#[no_mangle]
pub unsafe extern "C" fn anna_hashring_free(ring: *mut AnnaHashRing) {
    if !ring.is_null() {
        drop(Box::from_raw(ring));
    }
}

// ── Mutation ────────────────────────────────────────────────────────

/// Insert a server into the ring with `virtual_nodes` virtual entries.
///
/// Returns 0 on success, -1 if tid >= 50 (port group overflow).
///
/// # Safety
/// `ring` must be valid. `public_ip` and `private_ip` must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn anna_hashring_insert(
    ring: *mut AnnaHashRing,
    public_ip: *const c_char,
    private_ip: *const c_char,
    tid: u32,
    virtual_nodes: u32,
) -> i32 {
    if tid >= MAX_TID {
        return -1;
    }
    let ring = &mut *ring;
    let pub_ip = CStr::from_ptr(public_ip).to_str().unwrap_or("");
    let priv_ip = CStr::from_ptr(private_ip).to_str().unwrap_or("");
    ring.ring.insert(
        pub_ip,
        priv_ip,
        tid,
        ring.base_offset,
        virtual_nodes,
        ring.global,
    );
    0
}

/// Remove all entries for a server (identified by IPs and tid) from the ring.
///
/// # Safety
/// `ring` must be valid. `public_ip` and `private_ip` must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn anna_hashring_remove(
    ring: *mut AnnaHashRing,
    public_ip: *const c_char,
    private_ip: *const c_char,
    tid: u32,
) {
    let ring = &mut *ring;
    let pub_ip = CStr::from_ptr(public_ip).to_str().unwrap_or("");
    let priv_ip = CStr::from_ptr(private_ip).to_str().unwrap_or("");
    ring.ring.remove(pub_ip, priv_ip, tid);
}

// ── Query ───────────────────────────────────────────────────────────

/// Return the number of entries in the ring (including virtual nodes).
///
/// # Safety
/// `ring` must be valid.
#[no_mangle]
pub unsafe extern "C" fn anna_hashring_size(ring: *const AnnaHashRing) -> u32 {
    (*ring).ring.size() as u32
}

/// Find the responsible servers for a key with `rep_count` replicas.
///
/// Walks the ring clockwise from hash(key), collecting up to `rep_count`
/// unique servers (by IP, not by virtual node). Returns the number of
/// servers found. Results are written to the `out_servers` array.
///
/// The caller must allocate `out_servers` with at least `max_results` entries.
/// The caller must free the `public_ip` and `private_ip` strings in each
/// returned `ServerInfo` via `anna_string_free`.
///
/// # Safety
/// `ring`, `key`, and `out_servers` must be valid.
#[no_mangle]
pub unsafe extern "C" fn anna_responsible_servers(
    ring: *const AnnaHashRing,
    key: *const c_char,
    rep_count: u32,
    out_servers: *mut ServerInfo,
    max_results: u32,
) -> u32 {
    let ring_ref = &(*ring);
    let key_str = CStr::from_ptr(key).to_str().unwrap_or("");

    let servers = ring_ref
        .ring
        .find_responsible(key_str, rep_count, ring_ref.global);

    let count = servers.len().min(max_results as usize);
    for (i, st) in servers.iter().take(count).enumerate() {
        let info = &mut *out_servers.add(i);
        info.public_ip = to_c_string(st.public_ip());
        info.private_ip = to_c_string(st.private_ip());
        info.tid = st.tid();
    }

    count as u32
}

/// Get all unique servers in the ring.
///
/// Returns the count. Each entry in `out_servers` gets public_ip, private_ip, tid.
/// Caller must free strings via `anna_string_free`.
///
/// # Safety
/// `ring` and `out_servers` must be valid.
#[no_mangle]
pub unsafe extern "C" fn anna_hashring_get_unique_servers(
    ring: *const AnnaHashRing,
    out_servers: *mut ServerInfo,
    max_results: u32,
) -> u32 {
    let ring_ref = &(*ring);
    let servers = ring_ref.ring.get_unique_servers();

    let count = servers.len().min(max_results as usize);
    for (i, st) in servers.iter().take(count).enumerate() {
        let info = &mut *out_servers.add(i);
        info.public_ip = to_c_string(st.public_ip());
        info.private_ip = to_c_string(st.private_ip());
        info.tid = st.tid();
    }

    count as u32
}

/// Find responsible local thread IDs for a key with `rep_count` replicas.
///
/// Uses the local hasher. Returns count of thread IDs written to `out_tids`.
/// Returns 0 if the ring was created with `global = true` (wrong ring type).
///
/// # Safety
/// `ring`, `key`, and `out_tids` must be valid.
#[no_mangle]
pub unsafe extern "C" fn anna_responsible_local(
    ring: *const AnnaHashRing,
    key: *const c_char,
    rep_count: u32,
    out_tids: *mut u32,
    max_results: u32,
) -> u32 {
    let ring_ref = &(*ring);
    if ring_ref.global {
        return 0; // Wrong ring type for local lookup.
    }
    let key_str = CStr::from_ptr(key).to_str().unwrap_or("");

    let tids = ring_ref.ring.find_responsible_local(key_str, rep_count);

    let count = tids.len().min(max_results as usize);
    for (i, &tid) in tids.iter().take(count).enumerate() {
        *out_tids.add(i) = tid;
    }

    count as u32
}

// ── Hash functions (for use by C++ code that maintains its own ring) ─

/// Compute the global hash for a key: hash("GLOBAL" + key).
/// Returns a u64 hash value.
///
/// # Safety
/// `input` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn anna_hash_global(input: *const c_char) -> u64 {
    use anna_server_common::hash_ring::global_hash_key;
    let s = CStr::from_ptr(input).to_str().unwrap_or("");
    global_hash_key(s)
}

/// Compute the local hash for a key: hash(key).
/// Returns a u64 hash value.
///
/// # Safety
/// `input` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn anna_hash_local(input: *const c_char) -> u64 {
    use anna_server_common::hash_ring::local_hash_key;
    let s = CStr::from_ptr(input).to_str().unwrap_or("");
    local_hash_key(s)
}

/// Compute the global hash for a server thread virtual ID.
/// Input should be formatted as "GLOBAL<private_ip>:<tid>_<virtual_num>".
///
/// # Safety
/// `input` must be a valid C string.
#[no_mangle]
pub unsafe extern "C" fn anna_hash_global_thread(input: *const c_char) -> u64 {
    use anna_server_common::hash_ring::global_hash_key;
    let s = CStr::from_ptr(input).to_str().unwrap_or("");
    // The input already includes "GLOBAL" prefix from the caller.
    global_hash_key(s)
}

// ── String management ───────────────────────────────────────────────

/// Free a string allocated by the library.
///
/// # Safety
/// `s` must be a valid pointer returned by a library function, or null.
#[no_mangle]
pub unsafe extern "C" fn anna_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(std::ffi::CString::from_raw(s));
    }
}

// ── Internal helpers ────────────────────────────────────────────────

fn to_c_string(s: &str) -> *mut c_char {
    std::ffi::CString::new(s).unwrap_or_default().into_raw()
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn c_str(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    #[test]
    fn lifecycle() {
        unsafe {
            let ring = anna_hashring_new(true, 0);
            assert!(!ring.is_null());
            assert_eq!(anna_hashring_size(ring), 0);
            anna_hashring_free(ring);
        }
    }

    #[test]
    fn insert_and_size() {
        unsafe {
            let ring = anna_hashring_new(true, 0);
            let pub_ip = c_str("1.2.3.4");
            let priv_ip = c_str("10.0.0.1");
            anna_hashring_insert(ring, pub_ip.as_ptr(), priv_ip.as_ptr(), 0, 100);
            assert_eq!(anna_hashring_size(ring), 100);
            anna_hashring_free(ring);
        }
    }

    #[test]
    fn insert_and_remove() {
        unsafe {
            let ring = anna_hashring_new(true, 0);
            let pub_ip = c_str("1.2.3.4");
            let priv_ip = c_str("10.0.0.1");
            anna_hashring_insert(ring, pub_ip.as_ptr(), priv_ip.as_ptr(), 0, 100);
            assert_eq!(anna_hashring_size(ring), 100);

            anna_hashring_remove(ring, pub_ip.as_ptr(), priv_ip.as_ptr(), 0);
            assert_eq!(anna_hashring_size(ring), 0);
            anna_hashring_free(ring);
        }
    }

    #[test]
    fn responsible_servers_single_node() {
        unsafe {
            let ring = anna_hashring_new(true, 0);
            let pub_ip = c_str("1.2.3.4");
            let priv_ip = c_str("10.0.0.1");
            anna_hashring_insert(ring, pub_ip.as_ptr(), priv_ip.as_ptr(), 0, 3000);

            let key = c_str("test_key");
            let mut servers = vec![
                ServerInfo {
                    public_ip: std::ptr::null_mut(),
                    private_ip: std::ptr::null_mut(),
                    tid: 0,
                };
                4
            ];

            let count = anna_responsible_servers(ring, key.as_ptr(), 1, servers.as_mut_ptr(), 4);
            assert_eq!(count, 1);

            let s = &servers[0];
            let result_pub = CStr::from_ptr(s.public_ip).to_str().unwrap();
            assert_eq!(result_pub, "1.2.3.4");

            anna_string_free(s.public_ip);
            anna_string_free(s.private_ip);
            anna_hashring_free(ring);
        }
    }

    #[test]
    fn responsible_servers_multi_node() {
        unsafe {
            let ring = anna_hashring_new(true, 0);

            let ip1_pub = c_str("1.2.3.4");
            let ip1_priv = c_str("10.0.0.1");
            anna_hashring_insert(ring, ip1_pub.as_ptr(), ip1_priv.as_ptr(), 0, 3000);

            let ip2_pub = c_str("5.6.7.8");
            let ip2_priv = c_str("10.0.0.2");
            anna_hashring_insert(ring, ip2_pub.as_ptr(), ip2_priv.as_ptr(), 0, 3000);

            let key = c_str("test_key");
            let mut servers = vec![
                ServerInfo {
                    public_ip: std::ptr::null_mut(),
                    private_ip: std::ptr::null_mut(),
                    tid: 0,
                };
                4
            ];

            let count = anna_responsible_servers(ring, key.as_ptr(), 2, servers.as_mut_ptr(), 4);
            assert_eq!(count, 2);

            // Free strings.
            for i in 0..count as usize {
                anna_string_free(servers[i].public_ip);
                anna_string_free(servers[i].private_ip);
            }
            anna_hashring_free(ring);
        }
    }

    #[test]
    fn get_unique_servers() {
        unsafe {
            let ring = anna_hashring_new(true, 0);

            let ip1_pub = c_str("1.2.3.4");
            let ip1_priv = c_str("10.0.0.1");
            anna_hashring_insert(ring, ip1_pub.as_ptr(), ip1_priv.as_ptr(), 0, 100);

            let ip2_pub = c_str("5.6.7.8");
            let ip2_priv = c_str("10.0.0.2");
            anna_hashring_insert(ring, ip2_pub.as_ptr(), ip2_priv.as_ptr(), 0, 100);

            let mut servers = vec![
                ServerInfo {
                    public_ip: std::ptr::null_mut(),
                    private_ip: std::ptr::null_mut(),
                    tid: 0,
                };
                4
            ];

            let count = anna_hashring_get_unique_servers(ring, servers.as_mut_ptr(), 4);
            assert_eq!(count, 2);

            for i in 0..count as usize {
                anna_string_free(servers[i].public_ip);
                anna_string_free(servers[i].private_ip);
            }
            anna_hashring_free(ring);
        }
    }

    #[test]
    fn responsible_local_threads() {
        unsafe {
            // Local ring: 2 threads, each with 3000 virtual nodes.
            let ring = anna_hashring_new(false, 0);

            let ip = c_str("1.2.3.4");
            let priv_ip = c_str("10.0.0.1");
            anna_hashring_insert(ring, ip.as_ptr(), priv_ip.as_ptr(), 0, 3000);
            anna_hashring_insert(ring, ip.as_ptr(), priv_ip.as_ptr(), 1, 3000);

            let key = c_str("test_key");
            let mut tids = [0u32; 4];

            let count = anna_responsible_local(ring, key.as_ptr(), 1, tids.as_mut_ptr(), 4);
            assert_eq!(count, 1);
            assert!(tids[0] == 0 || tids[0] == 1);

            anna_hashring_free(ring);
        }
    }
}
