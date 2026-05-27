import re
with open("crates/memfuse-store/src/sstable.rs", "r") as f:
    code = f.read()

# 1. Replace try_into().unwrap()
code = code.replace(".try_into().unwrap()", '.try_into().map_err(|_| MemFuseError::Storage("invalid slice".into()))?')

# 2. Add tokio Mutex around file
code = code.replace("file: std::fs::File,", "file: tokio::sync::Mutex<tokio::fs::File>,")

# 3. Fix `open` method
open_old = """        // Read the actual first key from the first data block header
        // (index stores last_key per block, NOT first_key)
        let sync_file = std::fs::File::open(&path)
            .map_err(|e| MemFuseError::Storage(format!("Failed to open SSTable: {}", e)))?;
        let first_key = if !index.is_empty() {
            use std::io::{Read, Seek};
            let mut f = &sync_file;
            f.seek(std::io::SeekFrom::Start(index[0].1))?;
            let mut hdr = [0u8; 2];
            f.read_exact(&mut hdr)?;
            let k_len = u16::from_le_bytes(hdr) as usize;
            let mut k_buf = vec![0u8; k_len];
            f.read_exact(&mut k_buf)?;
            Bytes::from(k_buf)
        } else {
            Bytes::new()
        };

        Ok(Self {
            file: sync_file,"""
open_new = """        // Read the actual first key from the first data block header
        // (index stores last_key per block, NOT first_key)
        let mut sync_file = tokio::fs::File::open(&path)
            .await
            .map_err(|e| MemFuseError::Storage(format!("Failed to open SSTable: {}", e)))?;
        let first_key = if !index.is_empty() {
            sync_file.seek(tokio::io::SeekFrom::Start(index[0].1))
                .await
                .map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
            let mut hdr = [0u8; 2];
            sync_file.read_exact(&mut hdr)
                .await
                .map_err(|e| MemFuseError::Storage(format!("Read failed: {}", e)))?;
            let k_len = u16::from_le_bytes(hdr) as usize;
            let mut k_buf = vec![0u8; k_len];
            sync_file.read_exact(&mut k_buf)
                .await
                .map_err(|e| MemFuseError::Storage(format!("Read failed: {}", e)))?;
            Bytes::from(k_buf)
        } else {
            Bytes::new()
        };

        Ok(Self {
            file: tokio::sync::Mutex::new(sync_file),"""
code = code.replace(open_old, open_new)

# Fix pub fn get, iter, scan_prefix, scan_range async and read logic
meths = ["pub fn get", "pub fn iter", "pub fn scan_prefix", "pub fn scan_range"]
for m in meths:
    code = code.replace(m, m.replace("pub fn", "pub async fn"))

# Replace synchronous read with async lock + seek + read
read_old = """            use std::os::unix::fs::FileExt;
            let mut block_data = vec![0u8; (next_offset - offset) as usize];
            self.file
                .read_exact_at(&mut block_data, offset)
                .map_err(|e| MemFuseError::Storage(format!("SSTable scan read failed: {}", e)))?;"""
read_new = """            let mut block_data = vec![0u8; (next_offset - offset) as usize];
            {
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let mut file = self.file.lock().await;
                file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
                file.read_exact(&mut block_data).await.map_err(|e| MemFuseError::Storage(format!("SSTable scan read failed: {}", e)))?;
            }"""
code = code.replace(read_old, read_new)

read_old2 = """            use std::os::unix::fs::FileExt;

            let mut block_data = vec![0u8; (next_offset - offset) as usize];
            self.file
                .read_exact_at(&mut block_data, offset)
                .map_err(|e| MemFuseError::Storage(format!("SSTable scan read failed: {}", e)))?;"""
code = code.replace(read_old2, read_new)

read_old3 = """        use std::os::unix::fs::FileExt;
        let mut block_data = vec![0u8; (next_offset - offset) as usize];
        self.file
            .read_exact_at(&mut block_data, offset)
            .map_err(|e| MemFuseError::Storage(format!("SSTable read failed: {}", e)))?;"""
read_new3 = """        let mut block_data = vec![0u8; (next_offset - offset) as usize];
        {
            use tokio::io::{AsyncReadExt, AsyncSeekExt};
            let mut file = self.file.lock().await;
            file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
            file.read_exact(&mut block_data).await.map_err(|e| MemFuseError::Storage(format!("SSTable read failed: {}", e)))?;
        }"""
code = code.replace(read_old3, read_new3)

read_old4 = """        use std::os::unix::fs::FileExt;

        for idx in 0..self.index.len() {"""
read_new4 = """        for idx in 0..self.index.len() {"""
code = code.replace(read_old4, read_new4)

read_old5 = """            let mut block_data = vec![0u8; (next_offset - offset) as usize];
            self.file
                .read_exact_at(&mut block_data, offset)
                .map_err(|e| MemFuseError::Storage(format!("SSTable iter read failed: {}", e)))?;"""
read_new5 = """            let mut block_data = vec![0u8; (next_offset - offset) as usize];
            {
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let mut file = self.file.lock().await;
                file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
                file.read_exact(&mut block_data).await.map_err(|e| MemFuseError::Storage(format!("SSTable iter read failed: {}", e)))?;
            }"""
code = code.replace(read_old5, read_new5)

read_old6 = """            let mut block_data = vec![0u8; (next_offset - offset) as usize];
            self.file
                .read_exact_at(&mut block_data, offset)
                .map_err(|e| MemFuseError::Storage(format!("SSTable range read failed: {}", e)))?;"""
read_new6 = """            let mut block_data = vec![0u8; (next_offset - offset) as usize];
            {
                use tokio::io::{AsyncReadExt, AsyncSeekExt};
                let mut file = self.file.lock().await;
                file.seek(std::io::SeekFrom::Start(offset)).await.map_err(|e| MemFuseError::Storage(format!("Seek failed: {}", e)))?;
                file.read_exact(&mut block_data).await.map_err(|e| MemFuseError::Storage(format!("SSTable range read failed: {}", e)))?;
            }"""
code = code.replace(read_old6, read_new6)

# lsm.rs: need to fix sync get/iter calls to .await
with open("crates/memfuse-store/src/lsm.rs", "r") as f:
    lsm_code = f.read()

lsm_code = lsm_code.replace("sst.get(key)?", "sst.get(key).await?")
lsm_code = lsm_code.replace("sst.scan_prefix(prefix)?", "sst.scan_prefix(prefix).await?")
lsm_code = lsm_code.replace("sst.scan_range(start.map(|s| s), end.map(|e| e))?", "sst.scan_range(start.map(|s| s), end.map(|e| e)).await?")

# Also update compaction.rs since iter() is now async!
with open("crates/memfuse-store/src/compaction.rs", "r") as f:
    comp_code = f.read()

comp_code = comp_code.replace("sst.iter()?", "sst.iter().await?")

with open("crates/memfuse-store/src/sstable.rs", "w") as f:
    f.write(code)

with open("crates/memfuse-store/src/lsm.rs", "w") as f:
    f.write(lsm_code)

with open("crates/memfuse-store/src/compaction.rs", "w") as f:
    f.write(comp_code)
