# Hydro Common 

This repository is a shared repository for header files,
[protobuf definitions](https://developers.google.com/protocol-buffers/), and scripts. 

This README provides a brief overview of the contents of this repository. 
This repository will not change frequently and should only contain code that is used across multiple Hydro subprojects.

* `cmake`: This directory has three helpers that are useful for any CMake-based project: `CodeCoverage.cmake` 
  uses `lcov` and `gcov` to automatically generate coverage information; `DownloadProject.cmake` automatically 
  downloads and configured external C++ dependencies.
* `include`: A variety of Hydro C++ header files, including shared lattice definitions, a Anna KVS client, 
  shared `typedef`s and other utilities.
* `proto`: Project API-level protobuf definitions.
* `scripts`: Various helper scripts that install dependencies and simplify creating Travis build processes.
* `vendor`: CMake configuration for Hydro vendor dependencies (ZeroMQ, SPDLog, and Yaml-CPP). 
