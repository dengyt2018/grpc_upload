```shell
cargo +nightly build -Z build-std=std,panic_abort -Z build-std-features=panic_immediate_abort --bin server --bin client --release --target=x86_64-pc-windows-msvc

cargo run --bin server -- -a 127.0.0.1 -p 50051 --pem tls/server.pem --key tls/server.key
cargo run --bin client -- -s http://127.0.0.1 -p 50051 --tls tls/ca.pem --dir test/test_files
```