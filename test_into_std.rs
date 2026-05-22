use tokio::fs::File;
#[tokio::main]
async fn main() {
    let f = File::create("test.txt").await.unwrap();
    let _sf = f.into_std().await;
}
