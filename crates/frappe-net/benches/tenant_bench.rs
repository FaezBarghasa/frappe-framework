use std::time::Instant;

fn extract_tenant_id(host: &str) -> String {
    let host_no_port = host.split(':').next().unwrap_or(host);
    let tenant_id = if host_no_port == "localhost" || host_no_port == "127.0.0.1" {
        "default_site".to_string()
    } else {
        let parts: Vec<&str> = host_no_port.split('.').collect();
        if parts.len() > 1 {
            parts[0].to_string()
        } else {
            "default_site".to_string()
        }
    };

    tenant_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn main() {
    println!("Running tenant_bench...");

    let hosts = vec![
        "site1.local:8080",
        "localhost:8080",
        "my-tenant.frappecloud.com",
        "another.site.org",
        "127.0.0.1",
    ];

    let start = Instant::now();
    let num_iterations = 10_000;
    
    for _ in 0..num_iterations {
        for host in &hosts {
            let _ = extract_tenant_id(host);
        }
    }
    let duration = start.elapsed();
    let total_runs = num_iterations * hosts.len();

    println!("Success! Extracted subdomain {} times.", total_runs);
    println!("Total duration: {:?}", duration);
    
    let mean_time = duration / total_runs as u32;
    println!("Mean time per extraction: {:?}", mean_time);
    
    assert!(mean_time < std::time::Duration::from_micros(10), "Mean time is greater than 10µs!");
}
