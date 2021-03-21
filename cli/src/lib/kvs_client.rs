// #include "anna.pb.h"
// #include "common.hpp"
// #include "requests.hpp"

use std::time::{SystemTime, UNIX_EPOCH, Duration};
use zmq::Context;
use log::{info, debug};

use crate::config::Config;

pub type Address = String;
pub type Key = String;
pub type TimePoint = std::time::SystemTime;

use crate::threads::{UserRoutingThread, UserThread};
use crate::proto::anna::KeyTuple;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;

struct PendingRequest {
    tp: TimePoint,
    worker_addr: Address,
    // request:    KeyRequest
}

pub struct KVSClient {
    // the set of routing addresses outside the cluster
    routing_threads: Vec<UserRoutingThread>,
    // the current request id
    rid: usize,
    // the IP and port functions for this thread
    ut: UserThread,
    seed: u64,
    // A Random Number Generator
    rng: Pcg64,
    // the ZMQ context we use to create sockets
    context: Context,
    // cache for retrieved worker addresses organized by key
    key_address_cache: HashMap<Key, HashSet<Address>>,
    // GC timeout
    timeout: usize,
}

//     // cache for opened sockets
//     socket_cache: SocketCache,
//
//     // ZMQ receiving sockets
//     key_address_puller: zmq::socket_t,
//     response_puller: zmq::socket_t,
//
//     pollitems: Vec<zmq::pollitem_t>,
//
//     // keeps track of pending requests due to missing worker address
//     pending_request_map: Map<Key, (TimePoint, Vec<KeyRequest>)>,
//
//     // keeps track of pending get responses
//     pending_get_response_map: Map<Key, PendingRequest>,
//
//     // keeps track of pending put responses
//     pending_put_response_map: Map<Key, Map<string, PendingRequest>>


//     /*
//         addrs A vector of routing addresses.
//         routing_thread_count The number of thread sone ach routing node
//         ip My node's IP address
//         tid My client's thread ID
//         timeout Length of request timeouts in ms
//     */
//     pub fn new(
//             routing_threads: Vec<UserRoutingThread>,
//             ip: String,
//             tid: Option<usize>,
//             timeout: Option<usize>) -> Self {
//
//         let key_address_puller = zmq::socket_t(context, ZMQ_PULL);
//         let response_puller = zmq::socket_t(context, ZMQ_PULL);
//
//         pollitems = {
// //         {static_cast<void*>(key_address_puller_), 0, ZMQ_POLLIN, 0},
// //         {static_cast<void*>(response_puller_), 0, ZMQ_POLLIN, 0},
//         };
//
//         let client = KVSClient {
//             ut,
//             context,
//             socket_cache: SocketCache(&context_, ZMQ_PUSH),
//             key_address_puller,
//             response_puller,
//             routing_threads,
//             rid: 0,
//             pending_request_map: (),
//             pending_get_response_map: (),
//             pollitems,
//             seed,
//             key_address_cache: (),
//             pending_put_response_map: ()
//         };
//
//         // bind the two sockets we listen on
//         key_address_puller.bind(ut.key_address_bind_address());
//         response_puller.bind(ut.response_bind_address());
//
//         client
//     }

impl KVSClient {
    pub fn new(config: &Config, tid: Option<usize>, timeout: Option<usize>) -> Self {
        let tid = tid.unwrap_or(0);
        let thread_count = config.get_routing_thread_count();
        let routing_ips = config.get_routing_ips();
        let mut routing_threads = Vec::with_capacity(routing_ips.len() * thread_count);
        for address in routing_ips {
            for i in 0..thread_count {
                routing_threads.push(UserRoutingThread::new(address, i));
            }
        }

        let seed = Self::generate_seed(config.get_user_ip(), tid);
        info!("Random seed is {}.", seed);
        let rng = rand_pcg::Pcg64::seed_from_u64(seed);

        // socket_cache_(SocketCache(&context_, ZMQ_PUSH)),
        // key_address_puller_(zmq::socket_t(context_, ZMQ_PULL)),
        // response_puller_(zmq::socket_t(context_, ZMQ_PULL)),
        //
        // // bind the two sockets we listen on
        // key_address_puller_.bind(ut_.key_address_bind_address());
        // response_puller_.bind(ut_.response_bind_address());
        //
        // pollitems_ = {
        // {static_cast<void*>(key_address_puller_), 0, ZMQ_POLLIN, 0},
        // {static_cast<void*>(response_puller_), 0, ZMQ_POLLIN, 0},
        // };

        KVSClient {
            routing_threads,
            rid: 0,
            ut: UserThread::new(config.get_user_ip(), tid),
            seed,
            rng,
            context: zmq::Context::new(),
            key_address_cache: HashMap::new(),
            timeout: timeout.unwrap_or(10_000),
        }
    }

    /*
        Generate a random u64 seed from the time, ip address and thread id
     */
    fn generate_seed(ip: &Address, tid: usize) -> u64 {
        // Get the system time in ms since epoch as a u64 and initialize the seed with that
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH).unwrap_or(Duration::from_micros(42));
        let mut seed = since_the_epoch.as_secs() * 1000 +
            since_the_epoch.subsec_nanos() as u64 / 1_000_000;

        // Hash the string IP Address down to a u64
        let mut hasher = DefaultHasher::new();
        ip.hash(&mut hasher);
        // And add it to the seed
        seed += hasher.finish();
        // Add the thread id also
        seed += tid as u64;

        seed
    }

    /*
        Clears the key address cache held by this client.
     */
    pub fn clear_cache(&mut self) {
        self.key_address_cache.clear()
    }

    /*
        Return the ZMQ context used by this client.
    */
    pub fn get_context(&self) -> &Context {
        &self.context
    }

    /*
        Return the random seed used by this client.
    */
    pub fn get_seed(&self) -> u64 {
        self.seed
    }

    /*
      Generates a unique request ID. usize will overflow and start counting from
      zero again when MAX_INT is reached.
    */
    fn get_request_id(&mut self) -> String {
        self.rid += 1;
        format!("{}:{}_{}", self.ut.ip(), self.ut.tid(), self.rid)
    }

    /*
      Returns one random routing thread's key address connection address. If the
      client is running outside of the cluster (ie, it is querying the ELB),
      there's only one address to choose from.
    */
    fn get_routing_thread(&mut self) -> Address {
        // random index into threads array - from 0 upto but not including routing_threads.len()
        self.routing_threads[self.rng.gen_range(0..self.routing_threads.len())]
            .key_address_connect_address()
    }

    pub fn get(&self, _tokens: &[&str]) {
        // debug!("GET: {:?}", tokens);
        // let responses = self.receive_async();
        // while responses.size() == 0 {
        //   responses = self.receive_async();
        // }
        //
        // if responses.size() > 1 {
        //     error!("Error: received more than one response");
        // }
        //
        // assert(responses[0].tuples(0).lattice_type() == LatticeType::LWW);
        //
        // let lww_lattice: LWWPairLattice<String>  =
        //     deserialize_lww(responses[0].tuples(0).payload());
        // lww_lattice.reveal().value
    }

    pub fn get_causal(&self, tokens: &[&str]) {
        debug!("GET_CAUSAL: {:?}", tokens);
    }
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     if (responses.size() > 1) {
//       std::cout << "Error: received more than one response" << std::endl;
//     }
//
//     assert(responses[0].tuples(0).lattice_type() == LatticeType::MULTI_CAUSAL);
//
//     MultiKeyCausalLattice<SetLattice<string>> mkcl =
//         MultiKeyCausalLattice<SetLattice<string>>(to_multi_key_causal_payload(
//             deserialize_multi_key_causal(responses[0].tuples(0).payload())));
//
//     for (const auto &pair : mkcl.reveal().vector_clock.reveal()) {
//       std::cout << "{" << pair.first << " : "
//                 << std::to_string(pair.second.reveal()) << "}" << std::endl;
//     }
//
//     for (const auto &dep_key_vc_pair : mkcl.reveal().dependencies.reveal()) {
//       std::cout << dep_key_vc_pair.first << " : ";
//       for (const auto &vc_pair : dep_key_vc_pair.second.reveal()) {
//         std::cout << "{" << vc_pair.first << " : "
//                   << std::to_string(vc_pair.second.reveal()) << "}"
//                   << std::endl;
//       }
//     }
//
//     std::cout << *(mkcl.reveal().value.reveal().begin()) << std::endl;

    pub fn put(&self, tokens: &[&str]) {
        debug!("PUT: {:?}", tokens);
        //     Key key = v[1];
//     LWWPairLattice<string> val(
//         TimestampValuePair<string>(generate_timestamp(0), v[2]));
//
//     // Put async
//     string rid = client->put_async(key, serialize(val), LatticeType::LWW);
//
//     // Receive
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     KeyResponse response = responses[0];
//
//     if (response.response_id() != rid) {
//       std::cout << "Invalid response: ID did not match request ID!"
//                 << std::endl;
//     }
//     if (response.error() == AnnaError::NO_ERROR) {
//       std::cout << "Success!" << std::endl;
//     } else {
//       std::cout << "Failure!" << std::endl;
//     }
    }

    pub fn put_causal(&self, tokens: &[&str]) {
        debug!("PUT_CAUSAL: {:?}", tokens);
//     Key key = v[1];
//
//     MultiKeyCausalPayload<SetLattice<string>> mkcp;
//     // construct a test client id - version pair
//     mkcp.vector_clock.insert("test", 1);
//
//     // construct one test dependencies
//     mkcp.dependencies.insert(
//         "dep1", VectorClock(map<string, MaxLattice<unsigned>>({{"test1", 1}})));
//
//     // populate the value
//     mkcp.value.insert(v[2]);
//
//     MultiKeyCausalLattice<SetLattice<string>> mkcl(mkcp);
//
//     // Put async
//     string rid = client->put_async(key, serialize(mkcl), LatticeType::MULTI_CAUSAL);
//
//     // Receive
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     KeyResponse response = responses[0];
//
//     if (response.response_id() != rid) {
//       std::cout << "Invalid response: ID did not match request ID!"
//                 << std::endl;
//     }
//     if (response.error() == AnnaError::NO_ERROR) {
//       std::cout << "Success!" << std::endl;
//     } else {
//       std::cout << "Failure!" << std::endl;
//     }
    }

    pub fn put_set(&self, tokens: &[&str]) {
        debug!("PUT SET: {:?}", tokens);
        //     set<string> set;
//     for (int i = 2; i < v.size(); i++) {
//       set.insert(v[i]);
//     }
//
//     // Put async
//     string rid = client->put_async(v[1], serialize(SetLattice<string>(set)),
//                                    LatticeType::SET);
//
//     // Receive
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     KeyResponse response = responses[0];
//
//     if (response.response_id() != rid) {
//       std::cout << "Invalid response: ID did not match request ID!"
//                 << std::endl;
//     }
//     if (response.error() == AnnaError::NO_ERROR) {
//       std::cout << "Success!" << std::endl;
//     } else {
//       std::cout << "Failure!" << std::endl;
//     }
    }

    pub fn get_set(&self, tokens: &[&str]) {
        debug!("GET SET: {:?}", tokens);
        //     // Get Async
//     string serialized;
//
//     // Receive
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     SetLattice<string> latt = deserialize_set(responses[0].tuples(0).payload());
//     print_set(latt.reveal());
    }

    /*
     * When a server thread tells us to invalidate the cache for a key it's
     * because we likely have out of date information for that key; it sends us
     * the updated information for that key, and update our cache with that
     * information.
     */
    fn invalidate_cache_for_key(&mut self, key: &Key, _tuple: &KeyTuple) {
        self.key_address_cache.remove(key);
    }
}

// //
// //   /**
// //    * Issue an async PUT request to the KVS for a certain lattice typed value.
// //    */
// //   string put_async(const Key& key, const string& payload,
// //                    LatticeType lattice_type) {
// //     KeyRequest request;
// //     KeyTuple* tuple = prepare_data_request(request, key);
// //     request.set_type(RequestType::PUT);
// //     tuple->set_lattice_type(lattice_type);
// //     tuple->set_payload(payload);
// //
// //     try_request(request);
// //     return request.request_id();
// //   }
// //
// //   /**
// //    * Issue an async GET request to the KVS.
// //    */
// //   void get_async(const Key& key) {
// //     // we issue GET only when it is not in the pending map
// //     if (pending_get_response_map_.find(key) ==
// //         pending_get_response_map_.end()) {
// //       KeyRequest request;
// //       prepare_data_request(request, key);
// //       request.set_type(RequestType::GET);
// //
// //       try_request(request);
// //     }
// //   }
// //
// //   vector<KeyResponse> receive_async() {
// //     vector<KeyResponse> result;
// //     kZmqUtil->poll(0, &pollitems_);
// //
// //     if (pollitems_[0].revents & ZMQ_POLLIN) {
// //       string serialized = kZmqUtil->recv_string(&key_address_puller_);
// //       KeyAddressResponse response;
// //       response.ParseFromString(serialized);
// //       Key key = response.addresses(0).key();
// //
// //       if (pending_request_map_.find(key) != pending_request_map_.end()) {
// //         if (response.error() == AnnaError::NO_SERVERS) {
// //           log_->error(
// //               "No servers have joined the cluster yet. Retrying request.");
// //           pending_request_map_[key].first = std::chrono::system_clock::now();
// //
// //           query_routing_async(key);
// //         } else {
// //           // populate cache
// //           for (const Address& ip : response.addresses(0).ips()) {
// //             key_address_cache_[key].insert(ip);
// //           }
// //
// //           // handle stuff in pending request map
// //           for (auto& req : pending_request_map_[key].second) {
// //             try_request(req);
// //           }
// //
// //           // GC the pending request map
// //           pending_request_map_.erase(key);
// //         }
// //       }
// //     }
// //
// //     if (pollitems_[1].revents & ZMQ_POLLIN) {
// //       string serialized = kZmqUtil->recv_string(&response_puller_);
// //       KeyResponse response;
// //       response.ParseFromString(serialized);
// //       Key key = response.tuples(0).key();
// //
// //       if (response.type() == RequestType::GET) {
// //         if (pending_get_response_map_.find(key) !=
// //             pending_get_response_map_.end()) {
// //           if (check_tuple(response.tuples(0))) {
// //             // error no == 2, so re-issue request
// //             pending_get_response_map_[key].tp_ =
// //                 std::chrono::system_clock::now();
// //
// //             try_request(pending_get_response_map_[key].request_);
// //           } else {
// //             // error no == 0 or 1
// //             result.push_back(response);
// //             pending_get_response_map_.erase(key);
// //           }
// //         }
// //       } else {
// //         if (pending_put_response_map_.find(key) !=
// //                 pending_put_response_map_.end() &&
// //             pending_put_response_map_[key].find(response.response_id()) !=
// //                 pending_put_response_map_[key].end()) {
// //           if (check_tuple(response.tuples(0))) {
// //             // error no == 2, so re-issue request
// //             pending_put_response_map_[key][response.response_id()].tp_ =
// //                 std::chrono::system_clock::now();
// //
// //             try_request(pending_put_response_map_[key][response.response_id()]
// //                             .request_);
// //           } else {
// //             // error no == 0
// //             result.push_back(response);
// //             pending_put_response_map_[key].erase(response.response_id());
// //
// //             if (pending_put_response_map_[key].size() == 0) {
// //               pending_put_response_map_.erase(key);
// //             }
// //           }
// //         }
// //       }
// //     }
// //
// //     // GC the pending request map
// //     set<Key> to_remove;
// //     for (const auto& pair : pending_request_map_) {
// //       if (std::chrono::duration_cast<std::chrono::milliseconds>(
// //               std::chrono::system_clock::now() - pair.second.first)
// //               .count() > timeout_) {
// //         // query to the routing tier timed out
// //         for (const auto& req : pair.second.second) {
// //           result.push_back(generate_bad_response(req));
// //         }
// //
// //         to_remove.insert(pair.first);
// //       }
// //     }
// //
// //     for (const Key& key : to_remove) {
// //       pending_request_map_.erase(key);
// //     }
// //
// //     // GC the pending get response map
// //     to_remove.clear();
// //     for (const auto& pair : pending_get_response_map_) {
// //       if (std::chrono::duration_cast<std::chrono::milliseconds>(
// //               std::chrono::system_clock::now() - pair.second.tp_)
// //               .count() > timeout_) {
// //         // query to server timed out
// //         result.push_back(generate_bad_response(pair.second.request_));
// //         to_remove.insert(pair.first);
// //         invalidate_cache_for_worker(pair.second.worker_addr_);
// //       }
// //     }
// //
// //     for (const Key& key : to_remove) {
// //       pending_get_response_map_.erase(key);
// //     }
// //
// //     // GC the pending put response map
// //     map<Key, set<string>> to_remove_put;
// //     for (const auto& key_map_pair : pending_put_response_map_) {
// //       for (const auto& id_map_pair :
// //            pending_put_response_map_[key_map_pair.first]) {
// //         if (std::chrono::duration_cast<std::chrono::milliseconds>(
// //                 std::chrono::system_clock::now() -
// //                 pending_put_response_map_[key_map_pair.first][id_map_pair.first]
// //                     .tp_)
// //                 .count() > timeout_) {
// //           result.push_back(generate_bad_response(id_map_pair.second.request_));
// //           to_remove_put[key_map_pair.first].insert(id_map_pair.first);
// //           invalidate_cache_for_worker(id_map_pair.second.worker_addr_);
// //         }
// //       }
// //     }
// //
// //     for (const auto& key_set_pair : to_remove_put) {
// //       for (const auto& id : key_set_pair.second) {
// //         pending_put_response_map_[key_set_pair.first].erase(id);
// //       }
// //     }
// //
// //     return result;
// //   }
// //


//
// //   /**
// //    * A recursive helper method for the get and put implementations that tries
// //    * to issue a request at most trial_limit times before giving up. It  checks
// //    * for the default failure modes (timeout, errno == 2, and cache
// //    * invalidation). If there are no issues, it returns the set of responses to
// //    * the respective implementations for them to deal with. This is the same as
// //    * the above implementation of try_multi_request, except it only operates on
// //    * a single request.
// //    */
// //   void try_request(KeyRequest& request) {
// //     // we only get NULL back for the worker thread if the query to the routing
// //     // tier timed out, which should never happen.
// //     Key key = request.tuples(0).key();
// //     Address worker = get_worker_thread(key);
// //     if (worker.length() == 0) {
// //       // this means a key addr request is issued asynchronously
// //       if (pending_request_map_.find(key) == pending_request_map_.end()) {
// //         pending_request_map_[key].first = std::chrono::system_clock::now();
// //       }
// //       pending_request_map_[key].second.push_back(request);
// //       return;
// //     }
// //
// //     request.mutable_tuples(0)->set_address_cache_size(
// //         key_address_cache_[key].size());
// //
// //     send_request<KeyRequest>(request, socket_cache_[worker]);
// //
// //     if (request.type() == RequestType::GET) {
// //       if (pending_get_response_map_.find(key) ==
// //           pending_get_response_map_.end()) {
// //         pending_get_response_map_[key].tp_ = std::chrono::system_clock::now();
// //         pending_get_response_map_[key].request_ = request;
// //       }
// //
// //       pending_get_response_map_[key].worker_addr_ = worker;
// //     } else {
// //       if (pending_put_response_map_[key].find(request.request_id()) ==
// //           pending_put_response_map_[key].end()) {
// //         pending_put_response_map_[key][request.request_id()].tp_ =
// //             std::chrono::system_clock::now();
// //         pending_put_response_map_[key][request.request_id()].request_ = request;
// //       }
// //       pending_put_response_map_[key][request.request_id()].worker_addr_ =
// //           worker;
// //     }
// //   }
// //
// //   /**
// //    * A helper method to check for the default failure modes for a request that
// //    * retrieves a response. It returns true if the caller method should reissue
// //    * the request (this happens if errno == 2). Otherwise, it returns false. It
// //    * invalidates the local cache if the information is out of date.
// //    */
// //   bool check_tuple(const KeyTuple& tuple) {
// //     Key key = tuple.key();
// //     if (tuple.error() == 2) {
// //       log_->info(
// //           "Server ordered invalidation of key address cache for key {}. "
// //           "Retrying request.",
// //           key);
// //
// //       invalidate_cache_for_key(key, tuple);
// //       return true;
// //     }
// //
// //     if (tuple.invalidate()) {
// //       invalidate_cache_for_key(key, tuple);
// //
// //       log_->info("Server ordered invalidation of key address cache for key {}",
// //                  key);
// //     }
// //
// //     return false;
// //   }
// //

//
// //
// //   /**
// //    * Invalidate the key caches for any key that previously had this worker in
// //    * its cache. The underlying assumption is that if the worker timed out, it
// //    * might have failed, and so we don't want to rely on it being alive for both
// //    * the key we were querying and any other key.
// //    */
// //   void invalidate_cache_for_worker(const Address& worker) {
// //     vector<string> tokens;
// //     split(worker, ':', tokens);
// //     string signature = tokens[1];
// //     set<Key> remove_set;
// //
// //     for (const auto& key_pair : key_address_cache_) {
// //       for (const string& address : key_pair.second) {
// //         vector<string> v;
// //         split(address, ':', v);
// //
// //         if (v[1] == signature) {
// //           remove_set.insert(key_pair.first);
// //         }
// //       }
// //     }
// //
// //     for (const string& key : remove_set) {
// //       key_address_cache_.erase(key);
// //     }
// //   }
// //
// //   /**
// //    * Prepare a data request object by populating the request ID, the key for
// //    * the request, and the response address. This method modifies the passed-in
// //    * KeyRequest and also returns a pointer to the KeyTuple contained by this
// //    * request.
// //    */
// //   KeyTuple* prepare_data_request(KeyRequest& request, const Key& key) {
// //     request.set_request_id(get_request_id());
// //     request.set_response_address(ut_.response_connect_address());
// //
// //     KeyTuple* tp = request.add_tuples();
// //     tp->set_key(key);
// //
// //     return tp;
// //   }
// //
// //   /**
// //    * returns all the worker threads for the key queried. If there are no cached
// //    * threads, a request is sent to the routing tier. If the query times out,
// //    * NULL is returned.
// //    */
// //   set<Address> get_all_worker_threads(const Key& key) {
// //     if (key_address_cache_.find(key) == key_address_cache_.end() ||
// //         key_address_cache_[key].size() == 0) {
// //       if (pending_request_map_.find(key) == pending_request_map_.end()) {
// //         query_routing_async(key);
// //       }
// //       return set<Address>();
// //     } else {
// //       return key_address_cache_[key];
// //     }
// //   }
// //
// //   /**
// //    * Similar to the previous method, but only returns one (randomly chosen)
// //    * worker address instead of all of them.
// //    */
// //   Address get_worker_thread(const Key& key) {
// //     set<Address> local_cache = get_all_worker_threads(key);
// //
// //     // This will be empty if the worker threads are not cached locally
// //     if (local_cache.size() == 0) {
// //       return "";
// //     }
// //
// //     return *(next(begin(local_cache), rand_r(&seed_) % local_cache.size()));
// //   }
// //
// //
// //   /**
// //    * Send a query to the routing tier asynchronously.
// //    */
// //   void query_routing_async(const Key& key) {
// //     // define protobuf request objects
// //     KeyAddressRequest request;
// //
// //     // populate request with response address, request id, etc.
// //     request.set_request_id(get_request_id());
// //     request.set_response_address(ut_.key_address_connect_address());
// //     request.add_keys(key);
// //
// //     Address rt_thread = get_routing_thread();
// //     send_request<KeyAddressRequest>(request, socket_cache_[rt_thread]);
// //   }
// //
//
// //
// //   KeyResponse generate_bad_response(const KeyRequest& req) {
// //     KeyResponse resp;
// //
// //     resp.set_type(req.type());
// //     resp.set_response_id(req.request_id());
// //     resp.set_error(AnnaError::TIMEOUT);
// //
// //     KeyTuple* tp = resp.add_tuples();
// //     tp->set_key(req.tuples(0).key());
// //
// //     if (req.type() == RequestType::PUT) {
// //       tp->set_lattice_type(req.tuples(0).lattice_type());
// //       tp->set_payload(req.tuples(0).payload());
// //     }
// //
// //     return resp;
// //   }
// //
// // };
// }