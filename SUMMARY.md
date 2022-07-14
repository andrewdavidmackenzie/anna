# The `anna` book

[README.md](README.md)

# Building `anna`
- [Detailed instructions on building `anna`](docs/building-anna.md)

# Running `anna`
- [Instructions for running `anna`](docs/running.md)

# Oxidization of `anna`
Work has begun to convert anna to a full rust project, starting gingerly by writing a rust client library 
and cli [`anna`](client/rust/README.md).

# Clients and CLIS
In the `client` folder there are three clients, written in C++, rust and Python. The rust one is separated
into a library and a binary, that separation will also be done in the C++ and Python versions, allowing 
applications that with to interact with anna servers directly to link the library and call methods to do
so, or use the CLIs to interact with it "interactively" or from a command file.

## CLI Testing
Work will be done soon, to do some simple system testing of anna using the CLIs, where the same test will
be run using all the CLIs to interact with the server process, ensuring they all work and work the same.