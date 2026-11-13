use anyhow::{Result, bail, ensure};
use reqwest::{
    Client,
    dns::{Addrs, Name, Resolve, Resolving},
};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
    time::Duration,
};
use url::{Host, Url};

pub fn public_url(value: &str) -> Result<Url> {
    ensure!(value.len() <= 8192, "URL is too long");
    let url = Url::parse(value).map_err(|_| anyhow::anyhow!("Invalid URL"))?;
    ensure!(
        url.scheme() == "https",
        "Only public HTTPS URLs are allowed"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URL credentials are not allowed"
    );
    match url.host() {
        Some(Host::Ipv4(ip)) if is_public_ip(ip.into()) => {}
        Some(Host::Ipv6(ip)) if is_public_ip(ip.into()) => {}
        Some(Host::Domain(host)) if public_host(host) => {}
        _ => bail!("Only public HTTPS destinations are allowed"),
    }
    Ok(url)
}

fn public_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host.contains('.')
        && ![
            "localhost",
            "local",
            "internal",
            "home",
            "lan",
            "test",
            "invalid",
            "onion",
        ]
        .iter()
        .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

pub(crate) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113))
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return public_v4(v4);
    }
    let [a, b, _, _, _, _, _, _] = ip.segments();
    // Global unicast only. Exclude transition/documentation/reserved ranges.
    (a & 0xe000) == 0x2000
        && !(a == 0x2001 && (b < 0x0200 || b == 0x0db8))
        && a != 0x2002
        && !(a == 0x3fff && b < 0x1000)
}

#[derive(Debug)]
struct PublicResolver;
impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses =
                approved_addresses(tokio::net::lookup_host((name.as_str(), 0)).await?.collect())?;
            // The checked addresses are the addresses used by the connector: no second DNS lookup.
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    Client::builder()
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(PublicResolver))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(90))
        .timeout(Duration::from_secs(600))
        .user_agent("Melly/1.0")
}
pub(crate) fn client() -> Result<Client> {
    client_builder()
        .build()
        .map_err(|_| anyhow::anyhow!("Could not initialize secure provider transport"))
}

fn approved_addresses(
    addresses: Vec<std::net::SocketAddr>,
) -> std::io::Result<Vec<std::net::SocketAddr>> {
    if addresses.is_empty() || addresses.iter().any(|a| !is_public_ip(a.ip())) {
        return Err(std::io::Error::other("Destination is not public"));
    }
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolver_rejects_mixed_public_private_and_reserved_answers() {
        for answers in [
            vec!["8.8.8.8:443", "10.0.0.1:443"],
            vec!["[::ffff:127.0.0.1]:443"],
            vec!["169.254.169.254:443"],
        ] {
            let addresses = answers.iter().map(|s| s.parse().unwrap()).collect();
            assert!(approved_addresses(addresses).is_err());
        }
        let addresses = vec![
            "8.8.8.8:443".parse().unwrap(),
            "[2001:4860:4860::8888]:443".parse().unwrap(),
        ];
        assert_eq!(approved_addresses(addresses.clone()).unwrap(), addresses);
        assert!(approved_addresses(vec![]).is_err());
    }
}
