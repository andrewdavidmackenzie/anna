use kvs_client;

// #include "yaml-cpp/yaml.h"


//
// unsigned kRoutingThreadCount;
//
// ZmqUtil zmq_util;
// ZmqUtilInterface *kZmqUtil = &zmq_util;
//

// TODO change to use display() or debug() or {:?} of Set directly
// or implement Display for Set(String)
fn print_set(set: Set<String>) {
    print!("{ ");
    for string in set {
        print!("{} ");
    }
    println!("}");
}

// void handle_request(KvsClientInterface *client, string input) {
//   vector<string> v;
//   split(input, ' ', v);
//
//   if (v.size() == 0) {
//     std::exit(EXIT_SUCCESS);
//   }
//
//   if (v[0] == "GET") {
//     client->get_async(v[1]);
//
//     vector<KeyResponse> responses = client->receive_async();
//     while (responses.size() == 0) {
//       responses = client->receive_async();
//     }
//
//     if (responses.size() > 1) {
//       std::cout << "Error: received more than one response" << std::endl;
//     }
//
//     assert(responses[0].tuples(0).lattice_type() == LatticeType::LWW);
//
//     LWWPairLattice<string> lww_lattice =
//         deserialize_lww(responses[0].tuples(0).payload());
//     std::cout << lww_lattice.reveal().value << std::endl;
//   } else if (v[0] == "GET_CAUSAL") {
//     // currently this mode is only for testing purpose
//     client->get_async(v[1]);
//
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
//   } else if (v[0] == "PUT") {
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
//   } else if (v[0] == "PUT_CAUSAL") {
//     // currently this mode is only for testing purpose
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
//   } else if (v[0] == "PUT_SET") {
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
//   } else if (v[0] == "GET_SET") {
//     // Get Async
//     client->get_async(v[1]);
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
//   } else {
//     std::cout << "Unrecognized command " << v[0]
//               << ". Valid commands are GET, GET_SET, PUT, PUT_SET, PUT_CAUSAL, "
//               << "and GET_CAUSAL." << std::endl;
//     ;
//   }
// }
//
// // Read commands interactively from the terminal
// void run(KvsClientInterface *client) {
//   string input;
//   while (true) {
//     std::cout << "kvs> ";
//
//     getline(std::cin, input);
//     handle_request(client, input);
//   }
// }
//
// // Read commands from `filename` until EOF
// void run(KvsClientInterface *client, string filename) {
//   string input;
//   std::ifstream infile(filename);
//
//   while (getline(infile, input)) {
//     handle_request(client, input);
//   }
// }
//

// int main(int argc, char *argv[]) {
//   // There can be two or three options
//   // #0 - binary name
//   // #1 - config filename
//   // #2 - input file with commands
//   if (argc < 2 || argc > 3) {
//     std::cerr << "Usage: " << argv[0] << " conf-file <input-file>" << std::endl;
//     std::cerr
//         << "Filename is optional. Omit the filename to run in interactive mode."
//         << std::endl;
//     return 1;
//   }
//
 Config conf = Config::read(argv[1] /* filename */ );

//
//   vector<UserRoutingThread> threads;
//   for (Address addr : config.routing_ips()) {
//     for (unsigned i = 0; i < kRoutingThreadCount; i++) {
//       threads.push_back(UserRoutingThread(addr, i));
//     }
//   }
//
//   KvsClient client(threads, ip, 0, 10000);
//
//   if (argc == 2) {
//     run(&client);
//   } else {
//     run(&client, argv[2]);
//   }
// }
