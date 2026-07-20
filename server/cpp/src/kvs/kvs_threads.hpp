
//  Copyright 2019 U.C. Berkeley RISE Lab
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.

#ifndef KVS_INCLUDE_THREADS_HPP_
#define KVS_INCLUDE_THREADS_HPP_

#include "threads.hpp"
#include "types.hpp"

// The port on which KVS servers listen for new node announcments.
const unsigned kNodeJoinPort = 6000;

// The port on which KVS servers listen for node departures.
const unsigned kNodeDepartPort = 6050;

// The port on which KVS servers are asked to depart by the monitoring system.
const unsigned kSelfDepartPort = 6100;

// The port on which KVS servers listen for replication factor responses.
const unsigned kServerReplicationResponsePort = 6150;

// The port on which KVS servers listen for requests for data.
const unsigned kKeyRequestPort = 6200;

// The port on which KVS servers listen for gossip from other KVS nodes.
const unsigned kGossipPort = 6250;

// The port on which KVS servers listen for a replication factor change from
// the monitoring system.
const unsigned kServerReplicationChangePort = 6300;

// The port on which KVS servers listen for responses to a request for listing
// the keys cached at a function node.
const unsigned kCacheIpResponsePort = 7050;

// The port on which KVS servers listen for responses from management node to a
// request for the list of all existing function nodes.
const unsigned kManagementNodeResponsePort = 7100;

// The port on which routing servers listen for cluster membership requests.
const unsigned kSeedPort = 6350;

// The port on which routing servers listen for cluster membership changes.
const unsigned kRoutingNotifyPort = 6400;

// The port on which routing servers listen for replication factor responses.
const unsigned kRoutingReplicationResponsePort = 6500;

// The port on which routing servers listen for replication factor change
// announcements from the monitoring system.
const unsigned kRoutingReplicationChangePort = 6550;

// The port on which the monitoring system listens for cluster membership
// changes.
const unsigned kMonitoringNotifyPort = 6600;

// The port on which monitoring threads listen for KVS responses when
// retrieving metadata.
const unsigned kMonitoringResponsePort = 6650;

// The port on which the monitoring system waits for a response from KVS nodes
// after they have finished departing.
const unsigned kDepartDonePort = 6700;

// The port on which the monitoring nodes listens for performance feedback from
// clients.
const unsigned kFeedbackReportPort = 6750;

// The port on which benchmark nodes listen for triggers.
const unsigned kBenchmarkCommandPort = 6900;

// The port on which storage nodes retrieve their restart counts from the
// management system.
const unsigned kKopsRestartCountPort = 7000;

// The port on which KVS servers listen for direct cache registration messages.
const unsigned kCacheRegistrationPort = 7200;

// The port on which the management server will listen for requests for
// executor nodes.
const unsigned kKopsFuncNodesPort = 7002;

class ServerThread {
  Address public_ip_;
  Address public_base_;

  Address private_ip_;
  Address private_base_;

  unsigned tid_;
  unsigned virtual_num_;

public:
  ServerThread() {}
  ServerThread(Address public_ip, Address private_ip, unsigned tid)
      : public_ip_(public_ip), private_ip_(private_ip),
        private_base_("tcp://" + private_ip_ + ":"),
        public_base_("tcp://" + public_ip_ + ":"), tid_(tid) {}

  ServerThread(Address public_ip, Address private_ip, unsigned tid,
               unsigned virtual_num)
      : public_ip_(public_ip), private_ip_(private_ip),
        private_base_("tcp://" + private_ip_ + ":"),
        public_base_("tcp://" + public_ip_ + ":"), tid_(tid),
        virtual_num_(virtual_num) {}

  Address public_ip() const { return public_ip_; }

  Address private_ip() const { return private_ip_; }

  unsigned tid() const { return tid_; }

  unsigned virtual_num() const { return virtual_num_; }

  string id() const { return private_ip_ + ":" + std::to_string(tid_); }

  string virtual_id() const {
    return private_ip_ + ":" + std::to_string(tid_) + "_" +
           std::to_string(virtual_num_);
  }

  Address node_join_connect_address() const {
    return private_base_ + std::to_string(tid_ + kNodeJoinPort + kBaseOffset);
  }

  Address node_join_bind_address() const {
    return private_base_ + std::to_string(tid_ + kNodeJoinPort + kBaseOffset);
  }

  Address node_depart_connect_address() const {
    return private_base_ + std::to_string(tid_ + kNodeDepartPort + kBaseOffset);
  }

  Address node_depart_bind_address() const {
    return private_base_ + std::to_string(tid_ + kNodeDepartPort + kBaseOffset);
  }

  Address self_depart_connect_address() const {
    return private_base_ + std::to_string(tid_ + kSelfDepartPort + kBaseOffset);
  }

  Address self_depart_bind_address() const {
    return private_base_ + std::to_string(tid_ + kSelfDepartPort + kBaseOffset);
  }

  Address key_request_connect_address() const {
    return public_base_ + std::to_string(tid_ + kKeyRequestPort + kBaseOffset);
  }

  Address key_request_bind_address() const {
    return private_base_ + std::to_string(tid_ + kKeyRequestPort + kBaseOffset);
  }

  Address replication_response_connect_address() const {
    return private_base_ +
           std::to_string(tid_ + kServerReplicationResponsePort + kBaseOffset);
  }

  Address replication_response_bind_address() const {
    return private_base_ + std::to_string(tid_ + kServerReplicationResponsePort + kBaseOffset);
  }

  Address cache_ip_response_connect_address() const {
    return private_base_ + std::to_string(tid_ + kCacheIpResponsePort + kBaseOffset);
  }

  Address cache_ip_response_bind_address() const {
    return private_base_ + std::to_string(tid_ + kCacheIpResponsePort + kBaseOffset);
  }

  Address management_node_response_connect_address() const {
    return private_base_ + std::to_string(tid_ + kManagementNodeResponsePort + kBaseOffset);
  }

  Address management_node_response_bind_address() const {
    return private_base_ + std::to_string(tid_ + kManagementNodeResponsePort + kBaseOffset);
  }

  Address gossip_connect_address() const {
    return private_base_ + std::to_string(tid_ + kGossipPort + kBaseOffset);
  }

  Address gossip_bind_address() const {
    return private_base_ + std::to_string(tid_ + kGossipPort + kBaseOffset);
  }

  Address replication_change_connect_address() const {
    return private_base_ + std::to_string(tid_ + kServerReplicationChangePort + kBaseOffset);
  }

  Address replication_change_bind_address() const {
    return private_base_ + std::to_string(tid_ + kServerReplicationChangePort + kBaseOffset);
  }

  Address cache_registration_connect_address() const {
    return public_base_ + std::to_string(tid_ + kCacheRegistrationPort + kBaseOffset);
  }

  Address cache_registration_bind_address() const {
    return private_base_ + std::to_string(tid_ + kCacheRegistrationPort + kBaseOffset);
  }
};

inline bool operator==(const ServerThread &l, const ServerThread &r) {
  if (l.id().compare(r.id()) == 0) {
    return true;
  } else {
    return false;
  }
}

class RoutingThread {
  Address ip_;
  Address ip_base_;
  unsigned tid_;

public:
  RoutingThread() {}

  RoutingThread(Address ip, unsigned tid)
      : ip_(ip), tid_(tid), ip_base_("tcp://" + ip_ + ":") {}

  Address ip() const { return ip_; }

  unsigned tid() const { return tid_; }

  Address seed_connect_address() const {
    return ip_base_ + std::to_string(tid_ + kSeedPort + kBaseOffset);
  }

  Address seed_bind_address() const {
    return ip_base_ + std::to_string(tid_ + kSeedPort + kBaseOffset);
  }

  Address notify_connect_address() const {
    return ip_base_ + std::to_string(tid_ + kRoutingNotifyPort + kBaseOffset);
  }

  Address notify_bind_address() const {
    return ip_base_ + std::to_string(tid_ + kRoutingNotifyPort + kBaseOffset);
  }

  Address key_address_connect_address() const {
    return ip_base_ + std::to_string(tid_ + kKeyAddressPort + kBaseOffset);
  }

  Address key_address_bind_address() const {
    return ip_base_ + std::to_string(tid_ + kKeyAddressPort + kBaseOffset);
  }

  Address replication_response_connect_address() const {
    return ip_base_ + std::to_string(tid_ + kRoutingReplicationResponsePort + kBaseOffset);
  }

  Address replication_response_bind_address() const {
    return ip_base_ + std::to_string(tid_ + kRoutingReplicationResponsePort + kBaseOffset);
  }

  Address replication_change_connect_address() const {
    return ip_base_ + std::to_string(tid_ + kRoutingReplicationChangePort + kBaseOffset);
  }

  Address replication_change_bind_address() const {
    return ip_base_ + std::to_string(tid_ + kRoutingReplicationChangePort + kBaseOffset);
  }
};

class MonitoringThread {
  Address ip_;
  Address ip_base_;

public:
  MonitoringThread() {}
  MonitoringThread(Address ip) : ip_(ip), ip_base_("tcp://" + ip_ + ":") {}

  Address ip() const { return ip_; }

  Address notify_connect_address() const {
    return ip_base_ + std::to_string(kMonitoringNotifyPort + kBaseOffset);
  }

  Address notify_bind_address() const {
    return ip_base_ + std::to_string(kMonitoringNotifyPort + kBaseOffset);
  }

  Address response_connect_address() const {
    return ip_base_ + std::to_string(kMonitoringResponsePort + kBaseOffset);
  }

  Address response_bind_address() const {
    return ip_base_ + std::to_string(kMonitoringResponsePort + kBaseOffset);
  }

  Address depart_done_connect_address() const {
    return ip_base_ + std::to_string(kDepartDonePort + kBaseOffset);
  }

  Address depart_done_bind_address() const {
    return ip_base_ + std::to_string(kDepartDonePort + kBaseOffset);
  }

  Address feedback_report_connect_address() const {
    return ip_base_ + std::to_string(kFeedbackReportPort + kBaseOffset);
  }

  Address feedback_report_bind_address() const {
    return ip_base_ + std::to_string(kFeedbackReportPort + kBaseOffset);
  }
};

class BenchmarkThread {
public:
  BenchmarkThread() {}
  BenchmarkThread(Address ip, unsigned tid) : ip_(ip), tid_(tid) {}

  Address ip() const { return ip_; }

  unsigned tid() const { return tid_; }

  Address benchmark_command_address() const {
    return "tcp://" + ip_ + ":" + std::to_string(tid_ + kBenchmarkCommandPort + kBaseOffset);
  }

private:
  Address ip_;
  unsigned tid_;
};

inline string get_join_count_req_address(string management_ip) {
  return "tcp://" + management_ip + ":" + std::to_string(kKopsRestartCountPort + kBaseOffset);
}

inline string get_func_nodes_req_address(string management_ip) {
  return "tcp://" + management_ip + ":" + std::to_string(kKopsFuncNodesPort + kBaseOffset);
}

struct ThreadHash {
  std::size_t operator()(const ServerThread &st) const {
    return std::hash<string>{}(st.id());
  }
};
#endif // KVS_INCLUDE_THREADS_HPP_
