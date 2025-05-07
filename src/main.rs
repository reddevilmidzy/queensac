use regex::Regex;
use std::fs;
use std::path::Path;

fn extract_links_from_file<P: AsRef<Path>>(path: P) -> Vec<String> {
    let content = fs::read_to_string(&path).unwrap();
    let url_regex = Regex::new(r"https?://(www\.)?[-a-zA-Z0-9@:%._+~#=]{1,256}\.[a-zA-Z0-9()]{1,6}\b([-a-zA-Z0-9()@:%_+.~#?&/=]*)").unwrap();

    url_regex
        .find_iter(&content)
        .map(|mat| {
            let url = mat.as_str();
            url.trim_end_matches(&[')', '>', '.', ',', ';'][..])
                .to_string()
        })
        .collect()
}

#[derive(Debug, Eq, PartialEq)]
enum LinkCheckResult {
    Valid,
    Invalid(String),
}

fn check_link(url: &str) -> LinkCheckResult {
    let res = reqwest::blocking::get(url);
    match res {
        Ok(res) => {
            let status = res.status();
            if status.is_success() || status.is_redirection() {
                LinkCheckResult::Valid
            } else {
                LinkCheckResult::Invalid(format!("HTTP status code: {}", status))
            }
        }
        Err(e) => LinkCheckResult::Invalid(format!("Request error: {}", e)),
    }
}

fn main() {
    let file_path = "example.md";
    let links = extract_links_from_file(file_path);

    for link in links {
        let result = check_link(&link);
        match result {
            LinkCheckResult::Invalid(message) => {
                println!("유효하지 않은 링크: '{}', 실패 원인: {}", link, message);
            }
            _ => {}
        }
    }

    println!("Sacrifice THE QUEEN!!");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn extracts_links_from_file() -> Result<(), std::io::Error> {
        // 테스트용 파일 생성
        let test_file_path = "test_file.txt";
        let mut file = File::create(test_file_path)?;

        writeln!(
            file,
            "Visit https://example.com and https://rust-lang.org for more info."
        )?;

        // 함수 호출 및 결과 확인
        let links = extract_links_from_file(test_file_path);
        assert_eq!(
            links,
            vec![
                "https://example.com".to_string(),
                "https://rust-lang.org".to_string()
            ]
        );

        // 테스트 후 파일 삭제
        fs::remove_file(test_file_path)?;
        Ok(())
    }

    #[test]
    fn validate_link() {
        let link = "https://redddy.com";
        assert!(matches!(check_link(link), LinkCheckResult::Invalid(_)));
        let link = "https://lazypazy.tistory.com";
        assert_eq!(check_link(link), LinkCheckResult::Valid);
    }
}
