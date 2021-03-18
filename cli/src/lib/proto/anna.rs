// syntax = "proto3";
use crate::proto::shared;
use std::iter::Map;
use crate::proto::shared::KeyVersion;

// An enum to differentiate between different KVS requests.
enum RequestType {
    // A default type to capture unspecified requests.
    RT_UNSPECIFIED = 0,
    // A request to retrieve data from the KVS.
    GET = 1,
    // A request to put data into the KVS.
    PUT = 2,
}

enum LatticeType {
    // No lattice type specified
    NONE = 0,
    // Last-writer wins lattice
    LWW = 1,
    // Unordered set lattice
    SET = 2,
    // Single-key causal lattice
    SINGLE_CAUSAL = 3,
    // Multi-key causal lattice
    MULTI_CAUSAL = 4,
    // Ordered-set lattice
    ORDERED_SET = 5,
    // Priority lattice
    PRIORITY = 6,
}

enum AnnaError {
    // The request did not have an error.
    NO_ERROR = 0,
    // The requested key does not exist.
    KEY_DNE = 1,
    // The request was sent to the wrong thread, which is not responsible for the
    // key.
    WRONG_THREAD = 2,
    // The request timed out.
    TIMEOUT = 3,
    // The lattice type was not correctly specified or conflicted with an
    // existing key.
    LATTICE = 4,
    // This error is returned by the routing tier if no servers are in the
    // cluster.
    NO_SERVERS = 5,
}

// TODO see how this is made into a protobuf
// A protobuf to represent an individual key, both for requests and responses.
pub struct KeyTuple {
    // The key name for this request/response.
    key: String, // = 1;

    // The lattice type for this key. Only required for server responses and PUT requests.
    lattice_type: LatticeType, // = 2;

    // The error type specified by the server (see AnnaError).
    error: AnnaError, // = 3;

    // The data associated with this key.
    payload: Vec<u8>, // = 4;

    // The number of server addresses the client is aware of for a particular
    // key; used for DHT membership change optimization.
    address_cache_size: u32, // = 5;

    // A boolean set by the server if the client's address_cache_size does not
    // match the metadata stored by the server.
    invalidate: bool, // = 6;
}

// An individual GET or PUT request; each request can batch multiple keys.
struct KeyRequest {
    // The type of this request (see RequestType).
    rename_as_type: RequestType, // = 1; // TODO rename to type

    // A list of KeyTuples batched in this request.
    tuples: Vec<KeyTuple>, // = 2;

    // The IP-port pair at which the client is waiting for the server's response.
    response_address: String, // = 3;

    // A client-specific ID used to match asynchronous requests with responses.
    request_id: String, // = 4;
}

// A response to a KeyRequest.
struct KeyResponse {
    // The type of response being sent back to the client (see RequestType).
    rename_as_type: RequestType, // = 1; // TODO rename to type

    // The individual response pairs associated with this request. There is a
    // 1-to-1 mapping between these and the KeyTuples in the corresponding
    // KeyRequest.
    tuples: Vec<KeyTuple>, // = 2;

    // The request_id specified in the corresponding KeyRequest. Used to
    // associate asynchronous requests and responses.
    response_id: String, // = 3;

    // Any errors associated with the whole request. Individual tuple errors are
    // captured in the corresponding KeyTuple. This will only be set if the whole
    // request times out.
    error: AnnaError, // = 4;
}

// A request to the routing tier to retrieve server addresses corresponding to
// individual keys.
struct KeyAddressRequest {
    // The IP-port pair at which the client will await a response.
    response_address: String, // = 1;

    // The names of the requested keys.
    keys: Vec<String>, // = 2;

    // A unique ID used by the client to match asynchronous requests with responses.
    request_id: String, // = 3;
}

// A mapping from individual keys to the set of servers responsible for that key.
struct KeyAddress {
    // The specified key.
    key: String, // = 1;

    // The IPs of the set of servers responsible for this key.
    ips: Vec<String>, // = 2; // TODO use Address?
}

// A 1-to-1 response from the routing tier for individual KeyAddressRequests.
struct KeyAddressResponse {
    key_address: KeyAddress, // TODO sub message was embedded in here

    // A batch of responses for individual keys.
    addresses: Vec<KeyAddress>, // = 1;

    // An error reported by the routing tier. This should only ever be a timeout.
    error: AnnaError, // = 2;

    // A unique ID used by the client to match asynchronous requests with responses.
    response_id: String, // = 3;
}

// LATTICE SERIALIZATION

// Serialization of last-write wins lattices.
struct LWWValue {
    // The last-writer wins timestamp associated with this data.
    timestamp: u64, // = 1;

    // The actual data stored by this LWWValue.
    value: Vec<u8>, // = 2;
}

// Serialization of unordered set lattices.
struct SetValue {
    // An unordered set of values in this lattice.
    values: Vec<u8>, // = 1;
}

// Serialization of a single-key causal lattice.
struct SingleKeyCausalValue {
    // The vector clock for this key, which maps from unique client IDs to
    // monotonically increasing integers.
    vector_clock: Map<String, u32>, //  = 1;

    // The set of values associated with this causal value. There will only be
    // more than one here if there are multiple causally concurrent updates.
    values: Vec<u8>, // = 2;
}

// An individual multi-key causal lattice, along with its associated dependencies.
struct MultiKeyCausalValue {
    // The vector clock associated with this particular key.
    vector_clock: Map<String, u32>, //  = 1;

    // The mapping from keys to vector clocks for each of the direct causal
    // dependencies this key has.
    dependencies: Vec<KeyVersion>, //  = 2;

    // The set of potentially causally concurrent values for this key.
    values: Vec<u8>, // = 3;
}

// Serialization of lowest-priority-wins lattices.
// #[derive(message)] TODO
struct PriorityValue {
    // The priority associated with this data
    priority: f64, // = 1,
    // The actual data stored by this PriorityValue
    value: Vec<u8>, // = 2,
}
