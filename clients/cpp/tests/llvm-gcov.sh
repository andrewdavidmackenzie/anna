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

# On Linux the code is compiled with GCC, which needs gcov (from GCC).
# On macOS the code is compiled with Clang, which needs llvm-cov gcov.
if [ "$(uname -s)" = "Linux" ]; then
    LLVM_COV="gcov"
elif [ -x /opt/homebrew/opt/llvm/bin/llvm-cov ]; then
    LLVM_COV="/opt/homebrew/opt/llvm/bin/llvm-cov"
elif [ -x /usr/local/opt/llvm/bin/llvm-cov ]; then
    LLVM_COV="/usr/local/opt/llvm/bin/llvm-cov"
else
    LLVM_COV="gcov"
fi
exec "$LLVM_COV" gcov "$@"