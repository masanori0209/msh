use crate::error::Result;

pub fn run(args: &[String]) -> Result<i32> {
    let code = args
        .first()
        .map(|s| s.parse::<i32>().unwrap_or(1))
        .unwrap_or(0);
    Ok(code)
}
