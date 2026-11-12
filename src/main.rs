use clap::Parser;
use scree::Outcome;
use scree::cli::Cli;
use scree::identity::Env;

fn main() {
    let cli = Cli::parse();
    match scree::run(cli, &Env::from_process()) {
        Ok(Outcome::Print(out)) => println!("{out}"),
        Ok(Outcome::Help) => {
            print!("{}", scree::help_text());
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("{}", e.to_json());
            std::process::exit(e.exit_code());
        }
    }
}
