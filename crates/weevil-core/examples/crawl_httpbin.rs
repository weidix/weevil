use std::error::Error;

use robotstxt::DefaultMatcher;
use ureq::Agent;
use weevil_core::{HtmlTree, Selector};

const USER_AGENT: &str = "weevil-example/0.1";
const PAGE_URL: &str = "https://httpbin.example/forms/post";
const ROBOTS_URL: &str = "https://httpbin.example/robots.txt";

fn main() -> Result<(), Box<dyn Error>> {
    let agent: Agent = Agent::config_builder()
        .user_agent(USER_AGENT)
        .build()
        .into();

    let robots = agent.get(ROBOTS_URL).call()?.body_mut().read_to_string()?;
    let mut matcher = DefaultMatcher::default();
    if !matcher.one_agent_allowed_by_robots(&robots, USER_AGENT, PAGE_URL) {
        return Err(format!("robots.txt disallows crawling {PAGE_URL}").into());
    }

    let html = agent.get(PAGE_URL).call()?.body_mut().read_to_string()?;
    let tree = HtmlTree::parse(&html);

    let selector = Selector::parse("form input[name]")?;
    let inputs = selector.find(&tree)?;

    println!("Fetched: {PAGE_URL}");
    println!("Input names:");
    for node_id in inputs {
        if let Some(name) = tree.attr(node_id, "name") {
            println!("- {name}");
        }
    }

    Ok(())
}
