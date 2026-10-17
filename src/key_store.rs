const SERVICE_NAME: &str = "Sunday";
const ACCOUNT_NAME: &str = "assistant-api-key";

pub fn save(api_key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, ACCOUNT_NAME)
        .map_err(|error| format!("Could not access the Windows credential vault: {error}"))?;
    entry
        .set_password(api_key)
        .map_err(|error| format!("Could not securely store the API key: {error}"))
}

pub fn load() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, ACCOUNT_NAME)
        .map_err(|error| format!("Could not access the Windows credential vault: {error}"))?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!(
            "Could not read the API key from the credential vault: {error}"
        )),
    }
}
