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

#ifndef LATENCY_REPORTER_HPP
#define LATENCY_REPORTER_HPP

#include <string>
#include <vector>
#include "benchmark.pb.h"
#include "zmq/socket_cache.hpp"
#include "zmq/zmq_util.hpp"

const unsigned kFeedbackReportPort = 6953;

class LatencyReporter {
public:
    LatencyReporter(const std::vector<Address>& monitoring_ips,
                    unsigned base_offset, unsigned tid = 0);

    void report(double latency_us, double throughput,
                const std::vector<std::pair<std::string, double>>& key_latencies);

    void set_warmup(bool warmup);
    void finish();

private:
    std::string uid_;
    unsigned base_offset_;
    bool warmup_;
    std::vector<Address> monitoring_ips_;
    zmq::context_t context_;
    SocketCache pushers_;
};

#endif
