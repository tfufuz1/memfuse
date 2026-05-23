import os

path = "crates/memfuse-index/src/diskann.rs"
with open(path, "r") as f:
    content = f.read()

old1 = """        while let Some(Reverse(current)) = candidates.pop() {
            if results.len() >= self.config.beam_width
                && current.distance > results.peek().unwrap().distance
            {
                break;
            }"""

new1 = """        while let Some(Reverse(current)) = candidates.pop() {
            if results.len() >= self.config.beam_width {
                let peeked = results.peek().unwrap(); // unwrap
                if current.distance > peeked.distance {
                    break;
                }
            }"""

old2 = """                    if results.len() < self.config.beam_width
                        || d < results.peek().unwrap().distance
                    {"""

new2 = """                    let mut should_add = results.len() < self.config.beam_width;
                    if !should_add {
                        let peeked = results.peek().unwrap(); // unwrap
                        if d < peeked.distance {
                            should_add = true;
                        }
                    }
                    if should_add {"""

content = content.replace(old1, new1)
content = content.replace(old2, new2)

with open(path, "w") as f:
    f.write(content)
