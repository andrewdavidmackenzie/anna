#include "gtest/gtest.h"

class SimpleTest : public ::testing::Test {
protected:
  SimpleTest() { }
  virtual ~SimpleTest() { }
};

TEST_F(SimpleTest, Assign) {
    EXPECT_EQ(1, 1);
}