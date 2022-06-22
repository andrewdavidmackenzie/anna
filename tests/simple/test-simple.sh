#!/bin/bash


#  Copyright 2019 U.C. Berkeley RISE Lab
#
#  Licensed under the Apache License, Version 2.0 (the "License");
#  you may not use this file except in compliance with the License.
#  You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.

cargo run -- start > /dev/null 2>&1

echo "Running tests..."
./build/cli/anna-cli conf/anna-local.yml tests/simple/input > tests/simple/output

DIFF=$(diff tests/simple/output tests/simple/expected)

if [ "$DIFF" != "" ]; then
  echo "Output did not match expected output (tests/simple/expected.out). Diff:"
  echo "$DIFF"
  exit 1
else
  echo "Test succeeded!"
fi

# Cleanup
rm tests/simple/output

echo "Stopping local server..."
cargo run -- stop > /dev/null 2>&1

exit 0