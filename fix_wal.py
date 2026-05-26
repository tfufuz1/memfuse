path = "crates/memfuse-crypto/src/wal_crypto.rs"
with open(path, "r") as f:
    content = f.read()

content = content.replace('WalHmac::new(key).unwrap() // unwrap allowed', 'WalHmac::new(key).unwrap(); // unwrap allowed')

with open(path, "w") as f:
    f.write(content)
