// syntax = "proto3";

use std::iter::Map;

// An arbitrary set of strings; used for a variety of purposes across the system.
struct StringSet {
    // An unordered set of keys.
    keys: Vec<String>, // = 1;
}

// A message representing a pointer to a particular version of a particular key.
pub struct KeyVersion {
    // The name of the key we are referencing.
    key: String, // = 1;

    // A vector clock for the version of the key we are referencing.
    vector_clock: Map<String, u32> , // = 2;
}

// A wrapper message for a list of KeyVersions.
struct KeyVersionList {
    // The list of KeyVersion references.
    keys: Vec<KeyVersion>, // = 1;
}

