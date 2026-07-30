#include "kvs/server_utils.hpp"
#include <filesystem>
#include <fstream>

namespace fs = std::filesystem;

class DiskSerializerTest : public ::testing::Test {
protected:
  string disk_root_;
  unsigned tid_ = 0;

  void SetUp() override {
    disk_root_ = (fs::temp_directory_path() / ("anna_disk_test_" + std::to_string(getpid()))).string() + "/";
    fs::create_directories(disk_root_ + "disk_0");
  }

  void TearDown() override {
    fs::remove_all(disk_root_);
  }
};

TEST_F(DiskSerializerTest, LWWPutGet) {
  DiskLWWSerializer serializer(tid_, disk_root_);

  kvs::LWWValue lww;
  lww.set_timestamp(100);
  lww.set_value("hello");
  string payload;
  lww.SerializeToString(&payload);

  int size = serializer.put("test_key", payload);
  EXPECT_GT(size, 0);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("test_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::LWWValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.timestamp(), 100);
  EXPECT_EQ(decoded.value(), "hello");
}

TEST_F(DiskSerializerTest, LWWGetMissing) {
  DiskLWWSerializer serializer(tid_, disk_root_);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("nonexistent", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, LWWRemove) {
  DiskLWWSerializer serializer(tid_, disk_root_);

  kvs::LWWValue lww;
  lww.set_timestamp(1);
  lww.set_value("to_delete");
  string payload;
  lww.SerializeToString(&payload);
  serializer.put("del_key", payload);

  EXPECT_TRUE(serializer.remove("del_key"));

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("del_key", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, LWWRemoveNonexistent) {
  DiskLWWSerializer serializer(tid_, disk_root_);
  // remove() returns false when the file does not exist.
  EXPECT_FALSE(serializer.remove("does_not_exist"));
  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("does_not_exist", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, LWWLastWriterWins) {
  DiskLWWSerializer serializer(tid_, disk_root_);

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
  DiskSetSerializer serializer(tid_, disk_root_);

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
  DiskSetSerializer serializer(tid_, disk_root_);

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
  DiskPrioritySerializer serializer(tid_, disk_root_);

  kvs::PriorityValue pv;
  pv.set_priority(5.0);
  pv.set_value("test");
  string payload;
  pv.SerializeToString(&payload);
  int size = serializer.put("pri_single", payload);
  EXPECT_GT(size, 0);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("pri_single", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::PriorityValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.value(), "test");
  EXPECT_DOUBLE_EQ(decoded.priority(), 5.0);
}

TEST_F(DiskSerializerTest, PriorityLowestWins) {
  DiskPrioritySerializer serializer(tid_, disk_root_);

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

TEST_F(DiskSerializerTest, PriorityEqualKeepsExisting) {
  DiskPrioritySerializer serializer(tid_, disk_root_);

  kvs::PriorityValue pv1;
  pv1.set_priority(5.0);
  pv1.set_value("first");
  string p1;
  pv1.SerializeToString(&p1);
  serializer.put("pri_eq", p1);

  kvs::PriorityValue pv2;
  pv2.set_priority(5.0);
  pv2.set_value("second");
  string p2;
  pv2.SerializeToString(&p2);
  serializer.put("pri_eq", p2);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("pri_eq", error);
  kvs::PriorityValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.value(), "first");
}

TEST_F(DiskSerializerTest, OrderedSetPutGet) {
  DiskOrderedSetSerializer serializer(tid_, disk_root_);

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
  DiskSingleKeyCausalSerializer serializer(tid_, disk_root_);

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
  DiskMultiKeyCausalSerializer serializer(tid_, disk_root_);

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

// --- Tests for rejected-update branches (existing value kept) ---

TEST_F(DiskSerializerTest, LWWOlderTimestampRejected) {
  DiskLWWSerializer serializer(tid_, disk_root_);

  kvs::LWWValue lww1;
  lww1.set_timestamp(200);
  lww1.set_value("newer");
  string p1;
  lww1.SerializeToString(&p1);
  int size1 = serializer.put("lww_reject", p1);
  EXPECT_GT(size1, 0);

  // Put with an older timestamp — should be rejected, returns existing file size.
  kvs::LWWValue lww2;
  lww2.set_timestamp(100);
  lww2.set_value("older");
  string p2;
  lww2.SerializeToString(&p2);
  int size2 = serializer.put("lww_reject", p2);
  EXPECT_GT(size2, 0);
  EXPECT_EQ(size2, size1);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("lww_reject", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::LWWValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.value(), "newer");
}

TEST_F(DiskSerializerTest, PriorityHigherRejected) {
  DiskPrioritySerializer serializer(tid_, disk_root_);

  kvs::PriorityValue pv1;
  pv1.set_priority(1.0);
  pv1.set_value("low_priority");
  string p1;
  pv1.SerializeToString(&p1);
  int size1 = serializer.put("pri_reject", p1);
  EXPECT_GT(size1, 0);

  // Put with a higher (worse) priority — should be rejected.
  kvs::PriorityValue pv2;
  pv2.set_priority(10.0);
  pv2.set_value("high_priority");
  string p2;
  pv2.SerializeToString(&p2);
  int size2 = serializer.put("pri_reject", p2);
  EXPECT_GT(size2, 0);
  EXPECT_EQ(size2, size1);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("pri_reject", error);
  kvs::PriorityValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.value(), "low_priority");
}

// --- Tests for disk I/O error paths ---

TEST_F(DiskSerializerTest, DiskWriteOpenFailure) {
  // Point at a non-existent directory so fstream open fails.
  string bad_path = disk_root_ + "no_such_dir/file";
  kvs::LWWValue lww;
  lww.set_timestamp(1);
  lww.set_value("test");
  int result = disk_write(bad_path, lww);
  EXPECT_EQ(result, -1);
}

TEST_F(DiskSerializerTest, DiskReadParseFailure) {
  // Write garbage to a file, then attempt to read it as a protobuf.
  string path = disk_fname(disk_root_, tid_, "garbage_key");
  {
    std::ofstream out(path, std::ios::binary);
    out << "this is not a valid protobuf";
  }

  kvs::LWWValue value;
  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  bool ok = disk_read(path, value, error);
  EXPECT_FALSE(ok);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, DiskRemoveSuccess) {
  // Create a file and confirm disk_remove returns true.
  string path = disk_fname(disk_root_, tid_, "rm_key");
  { std::ofstream out(path); out << "data"; }
  EXPECT_TRUE(disk_remove(path));
}

TEST_F(DiskSerializerTest, DiskRemoveFailure) {
  // Attempt to remove a non-existent file.
  string path = disk_fname(disk_root_, tid_, "no_such_file");
  EXPECT_FALSE(disk_remove(path));
}

TEST_F(DiskSerializerTest, PutToNonexistentDirectory) {
  // Serializer with a bad disk_root: put should fail since directory doesn't exist.
  string bad_root = disk_root_ + "nonexistent_subdir/";
  DiskLWWSerializer serializer(tid_, bad_root);

  kvs::LWWValue lww;
  lww.set_timestamp(1);
  lww.set_value("fail");
  string payload;
  lww.SerializeToString(&payload);
  int result = serializer.put("key", payload);
  EXPECT_EQ(result, -1);
}

// --- Tests for remove on other Disk serializer types ---

TEST_F(DiskSerializerTest, SetRemove) {
  DiskSetSerializer serializer(tid_, disk_root_);

  kvs::SetValue sv;
  sv.add_values("a");
  string payload;
  sv.SerializeToString(&payload);
  serializer.put("set_rm", payload);

  EXPECT_TRUE(serializer.remove("set_rm"));

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("set_rm", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, SetRemoveNonexistent) {
  DiskSetSerializer serializer(tid_, disk_root_);
  EXPECT_FALSE(serializer.remove("no_such_set_key"));
}

TEST_F(DiskSerializerTest, OrderedSetRemove) {
  DiskOrderedSetSerializer serializer(tid_, disk_root_);

  kvs::SetValue sv;
  sv.add_values("x");
  string payload;
  sv.SerializeToString(&payload);
  serializer.put("oset_rm", payload);

  EXPECT_TRUE(serializer.remove("oset_rm"));

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("oset_rm", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, OrderedSetRemoveNonexistent) {
  DiskOrderedSetSerializer serializer(tid_, disk_root_);
  EXPECT_FALSE(serializer.remove("no_such_oset_key"));
}

TEST_F(DiskSerializerTest, SingleCausalRemove) {
  DiskSingleKeyCausalSerializer serializer(tid_, disk_root_);

  kvs::SingleKeyCausalValue skc;
  (*skc.mutable_vector_clock())["test"] = 1;
  skc.add_values("val");
  string payload;
  skc.SerializeToString(&payload);
  serializer.put("sc_rm", payload);

  EXPECT_TRUE(serializer.remove("sc_rm"));

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("sc_rm", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, SingleCausalRemoveNonexistent) {
  DiskSingleKeyCausalSerializer serializer(tid_, disk_root_);
  EXPECT_FALSE(serializer.remove("no_such_sc_key"));
}

TEST_F(DiskSerializerTest, MultiCausalRemove) {
  DiskMultiKeyCausalSerializer serializer(tid_, disk_root_);

  kvs::MultiKeyCausalValue mkc;
  (*mkc.mutable_vector_clock())["test"] = 1;
  mkc.add_values("val");
  string payload;
  mkc.SerializeToString(&payload);
  serializer.put("mc_rm", payload);

  EXPECT_TRUE(serializer.remove("mc_rm"));

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("mc_rm", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, MultiCausalRemoveNonexistent) {
  DiskMultiKeyCausalSerializer serializer(tid_, disk_root_);
  EXPECT_FALSE(serializer.remove("no_such_mc_key"));
}

TEST_F(DiskSerializerTest, PriorityRemove) {
  DiskPrioritySerializer serializer(tid_, disk_root_);

  kvs::PriorityValue pv;
  pv.set_priority(5.0);
  pv.set_value("to_delete");
  string payload;
  pv.SerializeToString(&payload);
  serializer.put("pri_rm", payload);

  EXPECT_TRUE(serializer.remove("pri_rm"));

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("pri_rm", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, PriorityRemoveNonexistent) {
  DiskPrioritySerializer serializer(tid_, disk_root_);
  EXPECT_FALSE(serializer.remove("no_such_pri_key"));
}

// Test that serializers append trailing slash to disk_root if missing.
TEST_F(DiskSerializerTest, DiskRootWithoutTrailingSlash) {
  // Remove trailing slash from disk_root.
  string root_no_slash = disk_root_;
  if (root_no_slash.back() == '/') {
    root_no_slash.pop_back();
  }
  // LWW serializer should work fine -- it appends '/' internally.
  DiskLWWSerializer lww_ser(tid_, root_no_slash);
  kvs::LWWValue lww;
  lww.set_timestamp(1);
  lww.set_value("slash_test");
  string payload;
  lww.SerializeToString(&payload);
  EXPECT_GT(lww_ser.put("slash_key", payload), 0);

  // Priority serializer should also work.
  DiskPrioritySerializer pri_ser(tid_, root_no_slash);
  kvs::PriorityValue pv;
  pv.set_priority(1.0);
  pv.set_value("pri_slash_test");
  string ppayload;
  pv.SerializeToString(&ppayload);
  EXPECT_GT(pri_ser.put("pri_slash_key", ppayload), 0);
}

// --- LWW_SET serializer tests ---

TEST(MemoryLWWSetSerializerTest, PutGetRoundtrip) {
  MemoryLWWKVS kvs;
  MemoryLWWSetSerializer serializer(&kvs);

  // Build a SetValue payload wrapped in LWWValue.
  kvs::SetValue sv;
  sv.add_values("alpha");
  sv.add_values("beta");
  string set_payload;
  sv.SerializeToString(&set_payload);

  kvs::LWWValue lww;
  lww.set_timestamp(100);
  lww.set_value(set_payload);
  string payload;
  lww.SerializeToString(&payload);

  int size = serializer.put("lww_set_key", payload);
  EXPECT_GT(size, 0);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("lww_set_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  // Decode outer LWWValue, then inner SetValue.
  kvs::LWWValue decoded_lww;
  decoded_lww.ParseFromString(result);
  EXPECT_EQ(decoded_lww.timestamp(), 100u);

  kvs::SetValue decoded_sv;
  decoded_sv.ParseFromString(decoded_lww.value());
  EXPECT_EQ(decoded_sv.values_size(), 2);
  set<string> values(decoded_sv.values().begin(), decoded_sv.values().end());
  EXPECT_TRUE(values.count("alpha"));
  EXPECT_TRUE(values.count("beta"));
}

TEST(MemoryLWWSetSerializerTest, GetMissingKeyReturnsKeyDne) {
  MemoryLWWKVS kvs;
  MemoryLWWSetSerializer serializer(&kvs);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("nonexistent", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST(MemoryLWWSetSerializerTest, LWWReplacesEntireSet) {
  MemoryLWWKVS kvs;
  MemoryLWWSetSerializer serializer(&kvs);

  // First PUT: {a, b}
  kvs::SetValue sv1;
  sv1.add_values("a");
  sv1.add_values("b");
  string sp1;
  sv1.SerializeToString(&sp1);
  kvs::LWWValue lww1;
  lww1.set_timestamp(100);
  lww1.set_value(sp1);
  string p1;
  lww1.SerializeToString(&p1);
  serializer.put("replace_key", p1);

  // Second PUT with higher timestamp: {x, y, z}
  kvs::SetValue sv2;
  sv2.add_values("x");
  sv2.add_values("y");
  sv2.add_values("z");
  string sp2;
  sv2.SerializeToString(&sp2);
  kvs::LWWValue lww2;
  lww2.set_timestamp(200);
  lww2.set_value(sp2);
  string p2;
  lww2.SerializeToString(&p2);
  serializer.put("replace_key", p2);

  // GET should return the second set (LWW replacement).
  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("replace_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::LWWValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.timestamp(), 200u);

  kvs::SetValue decoded_sv;
  decoded_sv.ParseFromString(decoded.value());
  EXPECT_EQ(decoded_sv.values_size(), 3);
  set<string> values(decoded_sv.values().begin(), decoded_sv.values().end());
  EXPECT_TRUE(values.count("x"));
  EXPECT_TRUE(values.count("y"));
  EXPECT_TRUE(values.count("z"));
  EXPECT_FALSE(values.count("a"));
}

TEST_F(DiskSerializerTest, LWWSetPutGet) {
  DiskLWWSetSerializer serializer(tid_, disk_root_);

  kvs::SetValue sv;
  sv.add_values("disk_val1");
  sv.add_values("disk_val2");
  string set_payload;
  sv.SerializeToString(&set_payload);

  kvs::LWWValue lww;
  lww.set_timestamp(42);
  lww.set_value(set_payload);
  string payload;
  lww.SerializeToString(&payload);

  int size = serializer.put("disk_lww_set", payload);
  EXPECT_GT(size, 0);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("disk_lww_set", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::LWWValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.timestamp(), 42u);

  kvs::SetValue decoded_sv;
  decoded_sv.ParseFromString(decoded.value());
  EXPECT_EQ(decoded_sv.values_size(), 2);
}

TEST_F(DiskSerializerTest, LWWSetGetMissing) {
  DiskLWWSetSerializer serializer(tid_, disk_root_);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("no_such_key", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST_F(DiskSerializerTest, LWWSetReplaces) {
  DiskLWWSetSerializer serializer(tid_, disk_root_);

  // First PUT
  kvs::SetValue sv1;
  sv1.add_values("old");
  string sp1;
  sv1.SerializeToString(&sp1);
  kvs::LWWValue lww1;
  lww1.set_timestamp(10);
  lww1.set_value(sp1);
  string p1;
  lww1.SerializeToString(&p1);
  serializer.put("disk_replace", p1);

  // Second PUT with higher timestamp
  kvs::SetValue sv2;
  sv2.add_values("new1");
  sv2.add_values("new2");
  string sp2;
  sv2.SerializeToString(&sp2);
  kvs::LWWValue lww2;
  lww2.set_timestamp(20);
  lww2.set_value(sp2);
  string p2;
  lww2.SerializeToString(&p2);
  serializer.put("disk_replace", p2);

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("disk_replace", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::LWWValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.timestamp(), 20u);

  kvs::SetValue decoded_sv;
  decoded_sv.ParseFromString(decoded.value());
  EXPECT_EQ(decoded_sv.values_size(), 2);
}

TEST_F(DiskSerializerTest, LWWSetOlderTimestampIgnored) {
  DiskLWWSetSerializer serializer(tid_, disk_root_);

  // PUT with timestamp 100
  kvs::SetValue sv1;
  sv1.add_values("newer");
  string sp1;
  sv1.SerializeToString(&sp1);
  kvs::LWWValue lww1;
  lww1.set_timestamp(100);
  lww1.set_value(sp1);
  string p1;
  lww1.SerializeToString(&p1);
  serializer.put("ts_key", p1);

  // PUT with older timestamp 50 — should be ignored
  kvs::SetValue sv2;
  sv2.add_values("older");
  string sp2;
  sv2.SerializeToString(&sp2);
  kvs::LWWValue lww2;
  lww2.set_timestamp(50);
  lww2.set_value(sp2);
  string p2;
  lww2.SerializeToString(&p2);
  serializer.put("ts_key", p2);

  // GET should return the first set (higher timestamp)
  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  string result = serializer.get("ts_key", error);
  EXPECT_EQ(error, kvs::AnnaError::NO_ERROR);

  kvs::LWWValue decoded;
  decoded.ParseFromString(result);
  EXPECT_EQ(decoded.timestamp(), 100u);

  kvs::SetValue decoded_sv;
  decoded_sv.ParseFromString(decoded.value());
  EXPECT_EQ(decoded_sv.values_size(), 1);
  EXPECT_EQ(decoded_sv.values(0), "newer");
}

TEST_F(DiskSerializerTest, LWWSetRemove) {
  DiskLWWSetSerializer serializer(tid_, disk_root_);

  kvs::SetValue sv;
  sv.add_values("to_delete");
  string sp;
  sv.SerializeToString(&sp);
  kvs::LWWValue lww;
  lww.set_timestamp(1);
  lww.set_value(sp);
  string payload;
  lww.SerializeToString(&payload);
  serializer.put("rm_key", payload);

  EXPECT_TRUE(serializer.remove("rm_key"));

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("rm_key", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}

TEST(MemoryLWWSetSerializerTest, RemoveKey) {
  MemoryLWWKVS kvs;
  MemoryLWWSetSerializer serializer(&kvs);

  kvs::SetValue sv;
  sv.add_values("val");
  string sp;
  sv.SerializeToString(&sp);
  kvs::LWWValue lww;
  lww.set_timestamp(1);
  lww.set_value(sp);
  string payload;
  lww.SerializeToString(&payload);
  serializer.put("rm_test", payload);

  EXPECT_TRUE(serializer.remove("rm_test"));

  kvs::AnnaError error = kvs::AnnaError::NO_ERROR;
  serializer.get("rm_test", error);
  EXPECT_EQ(error, kvs::AnnaError::KEY_DNE);
}
