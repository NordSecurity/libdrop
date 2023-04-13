use std::fmt::Display;

use base64::Engine;

pub struct Authorization {
    pub ticket: String,
    pub nonce: String,
}

pub struct WWWAuthenticate {
    pub nonce: String,
}

impl WWWAuthenticate {
    pub const KEY: &str = "www-authenticate";

    pub fn new(nonce: super::Nonce) -> Self {
        Self {
            nonce: super::BASE64.encode(nonce.0),
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let (scheme, value) = value
            .trim_start()
            .split_once(|c: char| c.is_ascii_whitespace())?;

        if scheme != super::AUTH_SCHEME {
            return None;
        };

        for split in value.split(',') {
            let (key, val) = split.trim().split_once('=')?;

            match key.trim_end() {
                "nonce" => {
                    return Some(Self {
                        nonce: val.trim_start().trim_matches('"').to_owned(),
                    });
                }
                _ => continue,
            };
        }

        None
    }
}

impl Display for WWWAuthenticate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} nonce=\"{}\"", super::AUTH_SCHEME, self.nonce)
    }
}

impl Authorization {
    pub const KEY: &str = "authorization";

    pub fn parse(value: &str) -> Option<Self> {
        let (scheme, value) = value
            .trim_start()
            .split_once(|c: char| c.is_ascii_whitespace())?;

        if scheme != super::AUTH_SCHEME {
            return None;
        };

        let mut ticket = None;
        let mut nonce = None;

        for split in value.split(',') {
            let (key, val) = split.trim().split_once('=')?;

            match key.trim_end() {
                "ticket" => ticket = Some(val.trim_start().trim_matches('"')),
                "nonce" => nonce = Some(val.trim_start().trim_matches('"')),
                _ => continue,
            };

            if let (Some(ticket), Some(nonce)) = (ticket, nonce) {
                return Some(Self {
                    ticket: ticket.to_owned(),
                    nonce: nonce.to_owned(),
                });
            }
        }

        None
    }
}

impl Display for Authorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            r#"{} ticket="{}", nonce="{}""#,
            super::AUTH_SCHEME,
            self.ticket,
            self.nonce,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization() {
        let v = r#"drop ticket=asdfasdf, nonce="jfjfjfjfjfjf""#;

        let a = Authorization::parse(v).unwrap();
        assert_eq!(a.ticket, "asdfasdf");
        assert_eq!(a.nonce, "jfjfjfjfjfjf");

        let v = r#"drop nonce="jfjfjfjfjfjf", ticket="asdfasdf""#;

        let a = Authorization::parse(v).unwrap();
        assert_eq!(a.ticket, "asdfasdf");
        assert_eq!(a.nonce, "jfjfjfjfjfjf");

        let v = r#"  drop       ticket =   "asdfasdf"  ,     nonce    =   "jfjfjfjfjfjf" ,  "#;

        let a = Authorization::parse(v).unwrap();
        assert_eq!(a.ticket, "asdfasdf");
        assert_eq!(a.nonce, "jfjfjfjfjfjf");

        let a = Authorization {
            ticket: String::from("asdfasdfasdf"),
            nonce: String::from("qwerttyuyuiu"),
        };
        let v = a.to_string();
        assert_eq!(v, r#"drop ticket="asdfasdfasdf", nonce="qwerttyuyuiu""#);
    }

    #[test]
    fn wwwauthenticate() {
        let v = r#"drop nonce="jfjfjfjfjfjf""#;

        let a = WWWAuthenticate::parse(v).unwrap();
        assert_eq!(a.nonce, "jfjfjfjfjfjf");

        let v = r#"drop nonce="jfjfjfjfjfjf", ticket="asdfasdf""#;

        let a = WWWAuthenticate::parse(v).unwrap();
        assert_eq!(a.nonce, "jfjfjfjfjfjf");

        let v = r#"  drop         nonce    =   "jfjfjfjfjfjf" ,  "#;

        let a = WWWAuthenticate::parse(v).unwrap();
        assert_eq!(a.nonce, "jfjfjfjfjfjf");

        let a = WWWAuthenticate {
            nonce: String::from("qwerttyuyuiu"),
        };
        let v = a.to_string();
        assert_eq!(v, r#"drop nonce="qwerttyuyuiu""#);
    }
}
