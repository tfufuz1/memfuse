path = "crates/memfuse-core/src/types/saos.rs"
with open(path, "r") as f:
    content = f.read()

content = content.replace('query.text_query.unwrap() // unwrap allowed // unwrap allowed', 'query.text_query.unwrap() // unwrap allowed')
content = content.replace('query.vector_query.unwrap() // unwrap allowed // unwrap allowed', 'query.vector_query.unwrap() // unwrap allowed')
content = content.replace('FusionWeights::new(0.4, 0.4, 0.1, 0.1).unwrap() // unwrap allowed // unwrap allowed;', 'FusionWeights::new(0.4, 0.4, 0.1, 0.1).unwrap(); // unwrap allowed')
content = content.replace('HybridQuery::builder().build().unwrap() // unwrap allowed // unwrap allowed;', 'HybridQuery::builder().build().unwrap(); // unwrap allowed')

with open(path, "w") as f:
    f.write(content)
