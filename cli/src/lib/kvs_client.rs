// #include "anna.pb.h"
// #include "common.hpp"
// #include "requests.hpp"
// #include "threads.hpp"

use log::info;
use std::iter::Map;

pub type Address = String;
pub type Key = String;
// let now = SystemTime::now();
pub type TimePoint = std::time::SystemTime;

struct PendingRequest {
    tp        : TimePoint,
    worker_addr:Address,
    // request:    KeyRequest
}

// struct KVSClient {
//     // the set of routing addresses outside the cluster
//     routing_threads: Vec<UserRoutingThread>,
//
//     // the current request id
//     rid: usize,
//
//     // the random seed for this client
//     seed: usize,
//
//     // the IP and port functions for this thread
//     ut: UserThread,
//
//     // the ZMQ context we use to create sockets
//     context: zmq::context_t,
//
//     // cache for opened sockets
//     socket_cache: SocketCache,
//
//     // ZMQ receiving sockets
//     key_address_puller: zmq::socket_t,
//     response_puller: zmq::socket_t,
//
//     pollitems: Vec<zmq::pollitem_t>,
//
//     // cache for retrieved worker addresses organized by key
//     key_address_cache: Map<Key, Set<Address>>,
//
//     // GC timeout
//     timeout: unsigned,
//
//     // keeps track of pending requests due to missing worker address
//     pending_request_map: Map<Key, (TimePoint, Vec<KeyRequest>)>,
//
//     // keeps track of pending get responses
//     pending_get_response_map: Map<Key, PendingRequest>,
//
//     // keeps track of pending put responses
//     pending_put_response_map: Map<Key, Map<string, PendingRequest>>
// }
//
// impl KVSClient {
//     /*
//         addrs A vector of routing addresses.
//         routing_thread_count The number of thread sone ach routing node
//         ip My node's IP address
//         tid My client's thread ID
//         timeout Length of request timeouts in ms
//     */
// //       log_(spdlog::basic_logger_mt("client_log", "client_log.txt", true)),
//     pub fn new(
//             routing_threads: Vec<UserRoutingThread>,
//             ip: String,
//             tid: Option<usize>,
//             timeout: Option<usize>) -> Self {
//
//         let context = zmq::context_t(1);
//         let key_address_puller = zmq::socket_t(context, ZMQ_PULL);
//         let response_puller = zmq::socket_t(context, ZMQ_PULL);
//
//         pollitems = {
// //         {static_cast<void*>(key_address_puller_), 0, ZMQ_POLLIN, 0},
// //         {static_cast<void*>(response_puller_), 0, ZMQ_POLLIN, 0},
//         };
//
//         let tid = tid.some_or(0);
//
// //     std::hash<string> hasher;
// //     seed_ = time(NULL);
// //     seed_ += hasher(ip);
// //     seed_ += tid;
//         info!("Random seed is {}.", seed);
//
//         let ut = UserThread(ip, tid);
//
//         let client = KVSClient {
//             ut,
//             context,
//             socket_cache: SocketCache(&context_, ZMQ_PUSH),
//             key_address_puller,
//             response_puller,
//             routing_threads,
//             rid: 0,
//             timeout: timeout.some_or(10000),
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
//     /*
//         Clears the key address cache held by this client.
//      */
//     pub fn clear_cache(&mut self) {
//         self.key_address_cache.clear()
//     }
//
//     /*
//         Return the ZMQ context used by this client.
//     */
//     pub fn get_context(&self) -> &zmq::context_t {
//         &self.context
//     }
//
//     /*
//         Return the random seed used by this client.
//     */
//     pub fn get_seed(&self) -> usize {
//         self.seed
//     }
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
//       /*
//        * When a server thread tells us to invalidate the cache for a key it's
//        * because we likely have out of date information for that key; it sends us
//        * the updated information for that key, and update our cache with that
//        * information.
//        */
//       fn invalidate_cache_for_key(&mut self, key: &Key, _tuple: &KeyTuple) {
//         self.key_address_cache.remove(key);
//       }
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
// //   /**
// //    * Returns one random routing thread's key address connection address. If the
// //    * client is running outside of the cluster (ie, it is querying the ELB),
// //    * there's only one address to choose from but 4 threads.
// //    */
// //   Address get_routing_thread() {
// //     return routing_threads_[rand_r(&seed_) % routing_threads_.size()]
// //         .key_address_connect_address();
// //   }
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
//       /*
//             Generates a unique request ID. usize will overflow and start counting from
//             zero again when MAX_INT is reached.
//       */
//       fn get_request_id(&mut self) -> String {
//         self.rid += 1;
//         format!("{}:{}_{}", self.ut.ip(), self.ut.tid(), self.rid)
//       }
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