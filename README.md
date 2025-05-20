# curlview

A modern Rust-based HTTP performance analyzer powered by curl. It visualizes timing metrics such as DNS lookup, TCP connection, TLS handshake, server processing, and content transfer in a human-readable format — similar to httpstat, but written in Rust.

## Features

- Visualize HTTP timing breakdowns
- Show IP address and port info
- Optional response body preview and saving
- Colored CLI output
- Fully configurable via environment variables
- Supports all curl options (except a few reserved flags)

## Installation

```bash
cargo install curlview
```

## Usage

```bash
curlview https://example.com [CURL_OPTIONS...]
```

### Example:

```bash
httpstat https://example.com -H "Accept: application/json"

IP Info: 192.168.0.1:50512  ⇄  93.184.216.34:443

  DNS Lookup   TCP Connection   TLS Handshake   Server Processing   Content Transfer
┌─────────────┬───────────────┬──────────────┬────────────────────┬─────────────────┐
│   12ms      │     48ms      │     20ms     │        130ms       │      40ms       │
└─────────────┴───────────────┴──────────────┴────────────────────┴─────────────────┘

Download: 235.6 KiB/s, Upload: 0.0 KiB/s
```


## Environment Variables

| Variable              | Description                          | Default  |
|-----------------------|--------------------------------------|----------|
| HTTPSTAT_SHOW_BODY    | Show response body in output         | false    |
| HTTPSTAT_SHOW_IP      | Display local/remote IP/port info    | true     |
| HTTPSTAT_SHOW_SPEED   | Show download/upload speed info      | false    |
| HTTPSTAT_SAVE_BODY    | Save response body to temp file      | true     |
| HTTPSTAT_CURL_BIN     | Custom curl binary path              | curl     |
| HTTPSTAT_DEBUG        | Print debug info                     | false    |
| HTTPSTAT_TIMEOUT      | Request timeout in seconds           | 10       |

## Disallowed curl flags

To maintain output consistency, the following flags are not allowed:

- `-w, --write-out`
- `-D, --dump-header`
- `-o, --output`
- `-s, --silent`
```

## License

MIT License
