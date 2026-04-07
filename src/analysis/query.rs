use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub enum Query {
    #[command(
        about = "Find the abstraction of an observation (use '<obs>@<seat>' for seat-aware abstractions)",
        alias = "abs"
    )]
    Abstraction {
        #[arg(required = true)]
        target: String,
    },

    #[command(
        about = "Find the distance between two targets (obs@seat~obs@seat or abs~abs)",
        alias = "dst"
    )]
    Distance {
        #[arg(required = true)]
        target1: String,
        #[arg(required = true)]
        target2: String,
    },

    #[command(
        about = "Find observations in the same cluster as a target (use '<obs>@<seat>' when needed)",
        alias = "sim"
    )]
    Similar {
        #[arg(required = true)]
        target: String,
    },

    #[command(
        about = "Find nearby abstractions for a target (use '<obs>@<seat>' when needed)",
        alias = "nbr"
    )]
    Nearby {
        #[arg(required = true)]
        target: String,
    },

    #[command(
        about = "Find the equity of a target (use '<obs>@<seat>' for seat-aware abstractions)",
        alias = "eqt"
    )]
    Equity {
        #[arg(required = true)]
        target: String,
    },

    #[command(
        about = "Find the population of a target (use '<obs>@<seat>' for seat-aware abstractions)",
        alias = "pop"
    )]
    Population {
        #[arg(required = true)]
        target: String,
    },

    #[command(
        about = "Find the histogram of a target (use '<obs>@<seat>' for seat-aware abstractions)",
        alias = "hst"
    )]
    Composition {
        #[arg(required = true)]
        target: String,
    },

    #[command(about = "Convert an integer to a Path representation", alias = "pth")]
    Path {
        #[arg(required = true)]
        value: i64,
    },

    #[command(about = "Convert an integer to an Edge representation", alias = "edg")]
    Edge {
        #[arg(required = true)]
        value: u8,
    },

    #[command(
        about = "Convert an integer to an Abstraction representation",
        alias = "abi"
    )]
    AbsFromInt {
        #[arg(required = true)]
        value: i64,
    },

    #[command(
        about = "Convert an integer to an Observation representation",
        alias = "obi"
    )]
    ObsFromInt {
        #[arg(required = true)]
        value: i64,
    },

    #[command(
        about = "Convert an integer to an Isomorphism representation",
        alias = "iso"
    )]
    Isomorphism {
        #[arg(required = true)]
        value: i64,
    },
}
