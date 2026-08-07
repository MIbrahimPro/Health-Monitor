use aegis_core::server::ApiServer;

#[tokio::main]
async fn main() {
    println!("Aegis Daemon running.");
    
    // In a real app this would be dynamically generated and persisted in config
    let shared_secret = "b4c97140".to_string();
    
    let server = ApiServer::new(shared_secret);
    server.start().await;
}
