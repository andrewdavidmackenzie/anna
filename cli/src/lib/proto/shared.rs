// syntax = "proto3";

// // An arbitrary set of strings; used for a variety of purposes across the
// // system.
// struct message StringSet {
// // An unordered set of keys.
// repeated string keys = 1;
// }
//
// // A message representing a pointer to a particular version of a particular
// // key.
// struct message KeyVersion {
// // The name of the key we are referencing.
// string key = 1;
//
// // A vector clock for the version of the key we are referencing.
// map<string, uint32> vector_clock = 2;
// }
//
// // A wrapper message for a list of KeyVersions.
// truct message KeyVersionList {
// // The list of KeyVersion references.
// repeated KeyVersion keys = 1;
// }
//
