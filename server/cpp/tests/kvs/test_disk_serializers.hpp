#include "kvs/server_utils.hpp"
#include <filesystem>
#include <fstream>

namespace fs = std::filesystem;

class DiskSerializerTest : public ::testing::Test {
protected:
  string ebs_root_;
  unsigned tid_ = 0;

  void SetUp() override {
    ebs_root_ = (fs::temp_directory_path() / ("anna_disk_test_" + std::to_string(getpid()))).string() + "/";
    fs::create_directories(ebs_root_ + "ebs_0");
  }

  void TearDown() override {
    fs::remove_all(ebs_root_);
  }
};

TEST_F(DiskSerializerTest, LWWPutGet) {
  DiskLWWSerializer serializer(tid_, ebs_root_);

  kvs::LWWValue lww;
  lww.set_timestamp(100);
  lww.set_value("hello");
  string payload;
  lww.SerializeToString(&payload);

  unsigned size = serializer.put("test_key", payload);
  EXPECT_GT(size, 0u);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("test_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::LWWValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.timestamp(), 100);
  EXPECT_EQ(decoded.value(), "hello");
}

TEST_F(DiskSerializerTest, LWWGetMissing) {
  DiskLWWSerializer serializer(tid_, ebs_root_);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("nonexistent", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, LWWRemove) {
  DiskLWWSerializer serializer(tid_, ebs_root_);

  kvs::LWWValue lww;
  lww.set_timestamp(1);
  lww.set_value("to_delete");
  string payload;
  lww.SerializeToString(&payload);
  serializer.put("del_key", payload);

  serializer.remove("del_key");

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("del_key", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, LWWLastWriterWins) {
  DiskLWWSerializer serializer(tid_, ebs_root_);

  kvs::LWWValue lww1;
  lww1.set_timestamp(100);
  lww1.set_value("first");
  string p1;
  lww1.SerializeToString(&p1);
  serializer.put("lww_key", p1);

  kvs::LWWValue lww2;
  lww2.set_timestamp(200);
  lww2.set_value("second");
  string p2;
  lww2.SerializeToString(&p2);
  serializer.put("lww_key", p2);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("lww_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::LWWValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.value(), "second");
}

TEST_F(DiskSerializerTest, SetPutGet) {
  DiskSetSerializer serializer(tid_, ebs_root_);

  kvs::SetValue sv;
  sv.add_values("a");
  sv.add_values("b");
  string payload;
  sv.SerializeToString(&payload);
  serializer.put("set_key", payload);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("set_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::SetValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.values_size(), 2);
}

TEST_F(DiskSerializerTest, SetUnionMerge) {
  DiskSetSerializer serializer(tid_, ebs_root_);

  kvs::SetValue sv1;
  sv1.add_values("a");
  sv1.add_values("b");
  string p1;
  sv1.SerializeToString(&p1);
  serializer.put("set_merge", p1);

  kvs::SetValue sv2;
  sv2.add_values("b");
  sv2.add_values("c");
  string p2;
  sv2.SerializeToString(&p2);
  serializer.put("set_merge", p2);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("set_merge", error);
  kvs::SetValue decoded;
  decoded.ParseFromString(result);
  EXPECT_GE(decoded.values_size(), 3);
}

TEST_F(DiskSerializerTest, PrioritySinglePutGet) {
  DiskPrioritySerializer serializer(tid_, ebs_root_);

  kvs::PriorityValue pv;
  pv.set_priority(5.0);
  pv.set_value("test");
  string payload;
  pv.SerializeToString(&payload);
  unsigned size = serializer.put("pri_single", payload);
  EXPECT_GT(size, 0u);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("pri_single", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::PriorityValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.value(), "test");
  EXPECT_DOUBLE_EQ(decoded.priority(), 5.0);
}

TEST_F(DiskSerializerTest, PriorityLowestWins) {
  DiskPrioritySerializer serializer(tid_, ebs_root_);

  kvs::PriorityValue pv1;
  pv1.set_priority(10.0);
  pv1.set_value("high");
  string p1;
  pv1.SerializeToString(&p1);
  serializer.put("pri_key", p1);

  kvs::PriorityValue pv2;
  pv2.set_priority(1.0);
  pv2.set_value("low");
  string p2;
  pv2.SerializeToString(&p2);
  serializer.put("pri_key", p2);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("pri_key", error);
  kvs::PriorityValue decoded;
  decoded.ParseFromString(result);
  EXPECT_LE(decoded.priority(), 1.0);
  EXPECT_EQ(decoded.value(), "low");
}

TEST_F(DiskSerializerTest, OrderedSetPutGet) {
  DiskOrderedSetSerializer serializer(tid_, ebs_root_);

  kvs::SetValue sv;
  sv.add_values("x");
  sv.add_values("y");
  string payload;
  sv.SerializeToString(&payload);
  serializer.put("oset_key", payload);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("oset_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);
}

TEST_F(DiskSerializerTest, SingleCausalPutGet) {
  DiskSingleKeyCausalSerializer serializer(tid_, ebs_root_);

  kvs::SingleKeyCausalValue skc;
  (*skc.mutable_vector_clock())["test"] = 1;
  skc.add_values("causal_val");
  string payload;
  skc.SerializeToString(&payload);
  serializer.put("sc_key", payload);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("sc_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);
}

TEST_F(DiskSerializerTest, MultiCausalPutGet) {
  DiskMultiKeyCausalSerializer serializer(tid_, ebs_root_);

  kvs::MultiKeyCausalValue mkc;
  (*mkc.mutable_vector_clock())["test"] = 1;
  mkc.add_values("mc_val");
  string payload;
  mkc.SerializeToString(&payload);
  serializer.put("mc_key", payload);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("mc_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);
}
