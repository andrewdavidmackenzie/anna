# Project

Anna is a low-latency, autoscaling key-value store (forked from
https://github.com/hydro-project/anna). It is a multi-language project with:
- C++ server and C++ client
- Python client
- Rust client (in clients/rust/)

The Rust client is one of several client implementations, not a rewrite of the entire system.


## General Considerations

1. Allow Claude to say "I don't know" if it can't find information to confirm a
   conclusion or answer, or can't quote sources for a statement when needed. I
   prefer no answer than one that may mislead us.

2. Verify with Citations. Make sure you can explain any conclusions you have reached
   by being able to cite the source information and then explain the logic used.

3. Use direct quotes for factual grounding.

## Build System

The project uses two build systems:
- **Makefile** (top-level) orchestrates the full build (C++ server, C++ client, Rust client)
- **Cargo** for the Rust client library
- **CMake** (via Makefile) for the C++ server and client

## Prefer Makefile targets over handmade scripts

The Makefile is the canonical way to build and test the project. Whenever possible, use an existing
Makefile target to get the job done.

If you see the need for a Makefile target that does not exist, but which would probably be used many times
for a specific task, then propose it to the user.

## Workflow Rules

- Never commit to master/main branch, always use a feature branch and create a PR.
- Always wait for code reviews to terminate or be repeated if they failed due to
  rate limiting, and then address all comments from the review.
- Always wait for the human user to approve before you merge a PR.
- After merging a PR, monitor the CI run on master. A PR is not considered
  successfully merged until master CI is green. If the merge breaks master CI,
  immediately fix it — do not leave master broken.
- Don't close GitHub issues without the user's explicit approval.
- Don't change Rust versions or install or uninstall anything using rustup without the user's explicit approval.
- Don't add new crate dependencies without the user's explicit approval.
- Explain your analysis of the problem and proposed implementation plan before starting to
  implement changes. Describe what files will be modified, what functions will be added/deleted/modified
- Always run `make clippy && make fmt && make test` before committing or pushing changes.

## General rust best practices
- keep visibility of structs and functions to the most private possible, pub – pub(crate) – private
- before adding a new struct or function, scan the code base for similar functions and structs and attempt to
  reuse them if they can be with minimal changes

## Coding Rules

- Use rust canonical code where possible. Implement `From` traits for conversion, create structs
  with methods, use traits when multiple implementations may be needed, etc.

## CI Notes

- Ubuntu CI requires `apt-get update` before `apt-get install` in the Makefile `dependencies`
  target. Without it, stale package mirror URLs cause 404 errors when Ubuntu rotates package
  versions (e.g. krb5 transitive dependencies).

## C++ Components

The project includes C++ server and client code alongside the Rust client:
- C++ code is built via CMake through the Makefile (`client-cpp` and `server-cpp` targets)
- C++ tests are run as part of `make test`
- Protocol buffer definitions live in `server/protobuf/` and are shared across languages
- Dependencies include: zmq, cppzmq, spdlog, yaml-cpp, googletest, protobuf, cmake

## Testing Rules

- Don't assume that any test failure is independent of your change. We start new features on a branch created from 
  master where tests were working. If in doubt check the history of GH Actions on the master branch
- Use `make test` not `cargo test` (make test covers C++ server, C++ client, and Rust tests)
- Don't modify any expected test output file to make a test pass without first showing a comparison 
  of the two to the user, or showing both side by side, and then the user explicitly approving the
  replacement of the old one with the new one.

## Committing and Pushing

- Never consider a task done, nor attempt to commit or push a change until make test passes.

## Merging
Never merge a PR to master without explicit user approval.
