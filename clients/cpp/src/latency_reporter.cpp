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

#include "latency_reporter.hpp"

LatencyReporter::LatencyReporter(
    const std::vector<Address>& monitoring_ips,
    unsigned base_offset, unsigned tid)
    : uid_("cpp_client:" + std::to_string(tid)),
      base_offset_(base_offset),
      warmup_(false),
      monitoring_ips_(monitoring_ips),
      context_(1),
      pushers_(&context_, ZMQ_PUSH) {}

void LatencyReporter::report(
    double latency_us, double throughput,
    const std::vector<std::pair<std::string, double>>& key_latencies) {
    UserFeedback feedback;
    feedback.set_uid(uid_);
    feedback.set_latency(latency_us);
    feedback.set_throughput(throughput);
    feedback.set_warmup(warmup_);

    for (const auto& [key, latency] : key_latencies) {
        auto* kl = feedback.add_key_latency();
        kl->set_key(key);
        kl->set_latency(latency);
    }

    std::string serialized;
    feedback.SerializeToString(&serialized);

    for (const auto& ip : monitoring_ips_) {
        Address addr = "tcp://" + ip + ":" + std::to_string(kFeedbackReportPort + base_offset_);
        kZmqUtil->send_string(serialized, &pushers_[addr]);
    }
}

void LatencyReporter::set_warmup(bool warmup) {
    warmup_ = warmup;
}

void LatencyReporter::finish() {
    UserFeedback feedback;
    feedback.set_uid(uid_);
    feedback.set_finish(true);

    std::string serialized;
    feedback.SerializeToString(&serialized);

    for (const auto& ip : monitoring_ips_) {
        Address addr = "tcp://" + ip + ":" + std::to_string(kFeedbackReportPort + base_offset_);
        kZmqUtil->send_string(serialized, &pushers_[addr]);
    }
}
