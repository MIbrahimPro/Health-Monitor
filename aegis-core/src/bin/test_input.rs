use rdev::{listen, Event};

fn callback(event: Event) {
    println!("My callback {:?}", event);
}

fn main() {
    println!("Listening for input events...");
    if let Err(error) = listen(callback) {
        println!("Error: {:?}", error)
    }
}
