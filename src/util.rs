
pub fn trim_lower_str_list(list: &[&str]) -> Vec<String> {
    list.iter()
        .map(|e| e.to_lowercase().trim().to_owned())
        .collect()
}