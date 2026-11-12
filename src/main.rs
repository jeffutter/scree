use clap::Parser;
use scree::cli::Cli;

fn main() {
    let cli = Cli::parse();
    match scree::run(cli) {
        Ok(out) => println!("{out}"),
        Err(e) => {
            eprintln!("{}", e.to_json());
            std::process::exit(e.exit_code());
        }
    }
}
