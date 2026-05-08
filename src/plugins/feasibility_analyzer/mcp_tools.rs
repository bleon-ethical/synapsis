//! MCP Tools for Feasibility Analyzer

use serde_json::{json, Value};

/// Analyze a vague idea into structured feasibility assessment
pub fn handle_feasibility_analyze(args: &Value) -> Result<Value, String> {
    let idea = args.get("idea").and_then(|v| v.as_str()).unwrap_or("");
    let domain = args
        .get("domain")
        .and_then(|v| v.as_str())
        .unwrap_or("general");
    let budget = args.get("budget").and_then(|v| v.as_str());
    let timeline = args.get("timeline").and_then(|v| v.as_str());

    if idea.is_empty() {
        return Err("Missing required field: 'idea'".to_string());
    }

    let complexity = assess_complexity(idea);
    let viability = assess_viability(idea);
    let risks = identify_risks(idea);

    Ok(json!({
        "idea": idea,
        "domain": domain,
        "assessment": {
            "technical_complexity": complexity.0,
            "complexity_weeks": complexity.1,
            "market_viability_score": viability.0,
            "viability_rationale": viability.1,
            "risk_level": risks.0,
            "top_risks": &risks.1[..std::cmp::min(3, risks.1.len())]
        },
        "requirements": {
            "must_have": extract_must_haves(idea),
            "nice_to_have": nice_to_haves(),
            "budget_estimate": budget.unwrap_or("not specified"),
            "timeline_estimate": timeline.unwrap_or("not specified")
        },
        "market_context": {
            "domain_maturity": domain_maturity(domain),
            "competition_level": competition_level(domain),
            "trend_direction": trend_direction(domain)
        },
        "next_steps": [
            "1. Validate market assumptions with target users (5-10 interviews)",
            format!("2. Build minimal prototype (est. {} weeks)", std::cmp::max(1, complexity.1 / 3)),
            "3. Run technical spike on highest-risk component",
            "4. Define MVP scope with stakeholder validation",
            "5. Persist findings in Synapsis memory for agent coordination"
        ],
        "generated_at": chrono::Utc::now().to_rfc3339()
    }))
}

/// Research current market trends for a domain
pub fn handle_market_trends(args: &Value) -> Result<Value, String> {
    let domain = args
        .get("domain")
        .and_then(|v| v.as_str())
        .unwrap_or("technology");
    let keywords = args.get("keywords").and_then(|v| v.as_str()).unwrap_or("");

    Ok(json!({
        "domain": domain,
        "keywords": keywords,
        "methodology": "Structured domain analysis + public data triangulation",
        "trends": [
            {
                "category": "Technology Adoption",
                "indicators": [
                    "Open-source ecosystem growth rate",
                    "Job market demand signals (LinkedIn/Indeed)",
                    "Conference/meetup activity trends"
                ],
                "analysis": format!("{}: monitor GitHub stars, Stack Overflow question velocity, and contributor growth as leading indicators.", domain)
            },
            {
                "category": "Market Signals",
                "indicators": [
                    "Venture capital investment patterns",
                    "Enterprise procurement trends",
                    "Regulatory landscape evolution"
                ],
                "analysis": "Cross-reference funding data with technical feasibility. Regulation can be either barrier or competitive moat."
            },
            {
                "category": "Competitive Landscape",
                "indicators": ["Incumbents", "Adjacent players", "New entrants"],
                "analysis": format!("Map existing solutions in {}. Identify gaps where technical complexity creates natural barriers.", domain)
            }
        ],
        "recommendation": "Validate via 10+ target user interviews before committing engineering resources. Use Synapsis task queue to parallelize research across agents.",
        "generated_at": chrono::Utc::now().to_rfc3339()
    }))
}

/// Generate a phased technical execution plan
pub fn handle_tech_plan(args: &Value) -> Result<Value, String> {
    let idea = args
        .get("idea")
        .and_then(|v| v.as_str())
        .unwrap_or("project");
    let team_size = args.get("team_size").and_then(|v| v.as_i64()).unwrap_or(2) as usize;

    let phases = vec![
        json!({
            "name": "Phase 1: Foundation",
            "weeks": 4,
            "focus": "Core technical prototype",
            "tasks": [
                format!("Set up infrastructure for {}", idea),
                "Build minimum viable core feature",
                "Establish CI/CD and monitoring",
                "Internal demo and technical review"
            ],
            "team": format!("{} engineers", team_size)
        }),
        json!({
            "name": "Phase 2: MVP",
            "weeks": 6,
            "focus": "User validation and iteration",
            "tasks": [
                "Integrate user feedback from prototype",
                "Add essential features based on feedback",
                "Performance optimization and hardening",
                "User acceptance testing"
            ],
            "team": format!("{} engineers + domain expert", team_size)
        }),
        json!({
            "name": "Phase 3: Production",
            "weeks": 4,
            "focus": "Launch readiness",
            "tasks": [
                "Security audit and penetration testing",
                "Production deployment with auto-scaling",
                "Documentation and runbooks",
                "Go-to-market preparation"
            ],
            "team": format!("{} engineers", team_size)
        }),
    ];

    Ok(json!({
        "execution_plan": {
            "phases": phases,
            "team_configuration": {
                "recommended_size": team_size,
                "roles": [
                    "Backend/Infrastructure Engineer",
                    "Domain Expert / Product Lead"
                ],
                "note": "Start lean. Add frontend/specialists after MVP validation."
            },
            "milestones": [
                {"phase": "Foundation", "deliverable": "Working prototype", "weeks": 4},
                {"phase": "MVP", "deliverable": "MVP with user feedback loop", "weeks": 6},
                {"phase": "Launch", "deliverable": "Production deployment", "weeks": 4}
            ],
            "go_no_go_criteria": [
                "Technical prototype works end-to-end",
                "At least 3 potential users express strong interest",
                "No blocking regulatory or legal issues found",
                "Core team committed for minimum 6 months"
            ]
        },
        "generated_at": chrono::Utc::now().to_rfc3339()
    }))
}

// Internal analysis helpers

fn assess_complexity(idea: &str) -> (&'static str, usize) {
    let lower = idea.to_lowercase();
    let indicators = [
        ("AI/ML", 8, "high"),
        ("blockchain", 8, "high"),
        ("real-time", 6, "medium-high"),
        ("distributed", 7, "high"),
        ("mobile app", 4, "medium"),
        ("web app", 3, "low-medium"),
        ("API", 2, "low"),
        ("CLI tool", 2, "low"),
        ("database", 3, "low-medium"),
        ("encryption", 6, "medium-high"),
        ("PQC", 8, "high"),
    ];
    let mut max_weeks = 4usize;
    let mut level = "medium";
    for (keyword, weeks, lvl) in &indicators {
        if lower.contains(keyword) {
            max_weeks = std::cmp::max(max_weeks, *weeks);
            level = lvl;
        }
    }
    (level, max_weeks)
}

fn assess_viability(idea: &str) -> (u8, String) {
    let lower = idea.to_lowercase();
    let mut score: u8 = 5;
    if lower.contains("AI") || lower.contains("machine learning") {
        score += 2;
    }
    if lower.contains("automation") {
        score += 1;
    }
    if lower.contains("security") {
        score += 1;
    }
    if lower.contains("crypto") && !lower.contains("scam") {
        score += 1;
    }
    if lower.contains("marketplace") {
        score = score.saturating_sub(1);
    }
    if lower.contains("social network") {
        score = score.saturating_sub(2);
    }
    if lower.contains("hardware") {
        score = score.saturating_sub(1);
    }
    score = score.clamp(0, 10);
    let rationale = match score {
        8..=10 => "Strong market demand signals + technical feasibility. High viability.",
        5..=7 => "Moderate viability. Requires key assumption validation before scaling.",
        0..=4 => "Significant market challenges. Consider pivoting or narrower scope.",
        _ => "Insufficient data.",
    };
    (score, rationale.to_string())
}

fn identify_risks(idea: &str) -> (&'static str, Vec<String>) {
    let lower = idea.to_lowercase();
    let mut items = Vec::new();
    let mut high = 0;
    if lower.contains("AI") || lower.contains("ML") {
        items.push("Model accuracy and data quality dependencies".into());
        high += 1;
    }
    if lower.contains("real-time") {
        items.push("Latency and reliability under load".into());
        high += 1;
    }
    if lower.contains("security") || lower.contains("auth") {
        items.push("Security vulnerabilities and compliance".into());
        high += 1;
    }
    if lower.contains("payment") || lower.contains("financial") {
        items.push("Regulatory compliance and financial liability".into());
        high += 1;
    }
    if lower.contains("marketplace") {
        items.push("Two-sided marketplace liquidity problem".into());
    }
    if lower.contains("scale") || lower.contains("million") {
        items.push("Scaling infrastructure costs vs revenue".into());
    }
    items.push("Engineering team availability and retention".into());
    items.push("Competing priorities and scope creep".into());
    (
        if high >= 2 {
            "high"
        } else if high >= 1 {
            "medium"
        } else {
            "low"
        },
        items,
    )
}

fn extract_must_haves(idea: &str) -> Vec<String> {
    let lower = idea.to_lowercase();
    let mut items = vec!["Core functional prototype".to_string()];
    if lower.contains("api") {
        items.push("REST/GraphQL API with authentication".into());
    }
    if lower.contains("database") || lower.contains("data") {
        items.push("Data persistence layer with backups".into());
    }
    if lower.contains("web") {
        items.push("Web interface (responsive)".into());
    }
    if lower.contains("mobile") {
        items.push("Mobile client (iOS or Android)".into());
    }
    if lower.contains("security") || lower.contains("auth") {
        items.push("Authentication + authorization system".into());
    }
    items.push("Automated test suite (unit + integration)".into());
    items
}

fn nice_to_haves() -> Vec<String> {
    vec![
        "Admin dashboard for operations".into(),
        "Usage analytics and monitoring".into(),
        "CI/CD pipeline with automated deploys".into(),
        "User documentation and onboarding flow".into(),
    ]
}

fn domain_maturity(domain: &str) -> &str {
    match domain.to_lowercase().as_str() {
        "ai" | "ml" | "artificial intelligence" => "Rapidly evolving, high investment",
        "web3" | "blockchain" | "crypto" => "Maturing after hype cycle correction",
        "cloud" | "infrastructure" => "Mature, commoditized",
        "security" | "cybersecurity" => "Evergreen, regulatory tailwinds",
        "devtools" | "developer tools" => "Growing, sticky user base",
        "healthtech" | "fintech" => "Regulated but high-value",
        _ => "Emerging — validate with primary research",
    }
}

fn competition_level(domain: &str) -> &str {
    match domain.to_lowercase().as_str() {
        "ai" | "ml" => "Extremely high — dominated by big tech",
        "web" | "mobile" => "High — low barrier to entry",
        "infrastructure" | "security" => "Moderate — technical moat possible",
        "devtools" => "Moderate — developer loyalty matters",
        _ => "Unknown — assess via direct research",
    }
}

fn trend_direction(domain: &str) -> &str {
    match domain.to_lowercase().as_str() {
        "ai" | "ml" => "Strong upward — AI adoption accelerating",
        "security" => "Upward — increasing threat surface",
        "web3" => "Consolidating — infrastructure over speculation",
        "devtools" => "Upward — developer experience premium",
        _ => "Neutral — requires domain-specific analysis",
    }
}
