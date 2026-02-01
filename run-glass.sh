#!/bin/bash
export ZED_COPY_REMOTE_SERVER="/Users/naaiyy/Developer/Glass-HQ/Glass/target/aarch64-apple-darwin/debug/remote_server.gz"
exec /Users/naaiyy/Developer/Glass-HQ/Glass/target/debug/Zed "$@"
