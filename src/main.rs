extern crate clap;
extern crate num_cpus;
extern crate threadpool;

use clap::{App, Arg};
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use threadpool::ThreadPool;
use warp::Filter;
use tokio::{runtime};

mod smtp;

struct Config {
    host: String,
    smtp_port: String,
    rest_port: u16,
}

impl Config {
    fn smtp_config(&self) -> String {
        format!(
            "{}:{}",
            self.host,
            self.smtp_port
        )
    }

    fn new(host: String, smtp_port: String, rest_port: u16) -> Config {
        Config { host: host, smtp_port: smtp_port, rest_port: rest_port }
    }
}

/// Parse the bind address from the command line arguments
fn parse_args() -> Config {
    const BIND_HOST_ARG_NAME: &str = "host";
    const BIND_PORT_PORT_NAME: &str = "port";
    const BIND_REST_PORT_PORT_NAME: &str = "rest-port";

    let matches = App::new("Rust SMTP server")
        .version("1.0")
        .author("Andreas Zitzelsberger <az@az82.de>")
        .about("Simple SMTP server that will print out messages received on stdout")
        .arg(
            Arg::with_name(BIND_HOST_ARG_NAME)
                .short("h")
                .help("Bind host")
                .default_value("localhost"),
        )
        .arg(
            Arg::with_name(BIND_PORT_PORT_NAME)
                .short("p")
                .help("Bind port")
                .default_value("2525")
                .validator(validate_port()),
        )
        .arg(
            Arg::with_name(BIND_REST_PORT_PORT_NAME)
                .short("r")
                .help("REST APIs port")
                .default_value("8080")
                .validator(validate_port()),
        )
        .get_matches();

    Config::new(
        matches.value_of(BIND_HOST_ARG_NAME).unwrap().to_string()
        , matches.value_of(BIND_PORT_PORT_NAME).unwrap().to_string()
        , matches.value_of(BIND_REST_PORT_PORT_NAME).unwrap().to_string().parse().unwrap(),
    )
}

fn validate_port() -> fn(String) -> Result<(), String> {
    |s: String| -> Result<(), String> {
        s.parse::<u16>()
            .and(Ok(()))
            .map_err(|e: std::num::ParseIntError| -> String { e.to_string() })
    }
}

/// Handle a client connection.
/// If the SMTP communication was successful, print a list of messages on stdout.
fn handle_connection(mut stream: TcpStream, repo_clone: Arc<Mutex<Vec<smtp::Connection>>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());

    match smtp::Connection::handle(&mut reader, &mut stream) {
        Ok(result) => {
            println!("Sender domain: {}", result.get_sender_domain().unwrap());
            for message in result.get_messages().unwrap() {
                println!("Message from: {}", message.get_sender());
                println!("To: {}", message.get_recipients().join(", "));
                println!("{}", message.get_data());
            }
            let mut repo = repo_clone.lock().unwrap();
            repo.push(result);
        }
        Err(e) => eprintln!("Error communicating with client: {}", e),
    }
}

fn main() {
    let mail_repository = Arc::new(Mutex::new(Vec::<smtp::Connection>::new()));
    let config = parse_args();
    println!("REST Port: {}", config.rest_port);

    let pool = ThreadPool::new(num_cpus::get());
    start_rest_server(&mail_repository, &config, &pool);
    start_smtp_server(mail_repository, &config, pool)
}

fn start_smtp_server(mail_repository: Arc<Mutex<Vec<smtp::Connection>>>, config: &Config, pool: ThreadPool) {
    let bind_address = config.smtp_config();
    let listener = TcpListener::bind(&bind_address)
        .unwrap_or_else(|e| panic!("Binding to {} failed: {}", &bind_address, e));


    for stream_result in listener.incoming() {
        let repo_clone = mail_repository.clone();
        match stream_result {
            Ok(stream) => pool.execute(|| {
                handle_connection(stream, repo_clone);
            }),
            Err(e) => eprintln!("Unable to handle client connection: {}", e),
        }
    }
}

fn start_rest_server(mail_repository: &Arc<Mutex<Vec<smtp::Connection>>>, config: &Config, pool: &ThreadPool) {
    let count_clone = mail_repository.clone();
    let get = warp::get().map(move || {
        let repo = count_clone.lock().unwrap();
        let response = smtp::ConnectionsResponse::new(repo.clone());
        warp::reply::json(&response)
    });

    let delete_clone = mail_repository.clone();
    let delete = warp::delete().map(move || {
        let mut repo = delete_clone.lock().unwrap();
        repo.clear();
        "Wiped"
    });

    let routes = get.or(delete);
    let ret = runtime::Builder::new_current_thread().enable_all().build();
    let port = config.rest_port;
    pool.execute(move || {
        ret.unwrap().block_on(warp::serve(routes).run(([127, 0, 0, 1], port)));
    });
}
