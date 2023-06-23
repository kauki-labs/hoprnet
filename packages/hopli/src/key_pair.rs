use crate::utils::HelperErrors;
use hoprd_keypair::key_pair::HoprKeys;
use log::warn;
use std::{fs, path::PathBuf};

/// Decrypt identity files and returns an vec of PeerIds and Ethereum Addresses
///
/// # Arguments
///
/// * `identity_directory` - Directory to all the identity files
/// * `password` - Password to unlock all the identity files
/// * `identity_prefix` - Prefix of identity files. Only identity files with the provided are decrypted with the password
pub fn read_identities(files: Vec<PathBuf>, password: &String) -> Result<Vec<HoprKeys>, HelperErrors> {
    let mut results: Vec<HoprKeys> = Vec::with_capacity(files.len());

    for file in files.iter() {
        let file_str = file
            .to_str()
            .ok_or(HelperErrors::IncorrectFilename(file.to_string_lossy().to_string()))?;

        println!("{}", file_str);

        match HoprKeys::read_eth_keystore(file_str, password) {
            Ok((keys, needs_migration)) => {
                if needs_migration {
                    keys.write_eth_keystore(file_str, password, false)?
                }
                results.push(keys)
            }
            Err(e) => {
                warn!("Could not decrypt keystore file at {}. {}", file_str, e.to_string())
            }
        }
    }

    Ok(results)
}

/// Create one identity file and return the ethereum address
///
/// # Arguments
///
/// * `dir_name` - Directory to the storage of an identity file
/// * `password` - Password to encrypt the identity file
/// * `name` - Prefix of identity files.
pub fn create_identity(dir_name: &str, password: &str, maybe_name: &Option<String>) -> Result<HoprKeys, HelperErrors> {
    // create dir if not exist
    fs::create_dir_all(dir_name)?;

    let keys = HoprKeys::new();

    // check if `name` is end with `.id`, if not, append it
    let file_path = match maybe_name {
        Some(name) => {
            // check if ending with `.id`
            if name.ends_with(".id") {
                format!("{dir_name}/{name}")
            } else {
                format!("{dir_name}/{name}.id")
            }
        }
        None => format!("{dir_name}/{}.id", { keys.id.to_string() }),
    };

    keys.write_eth_keystore(&file_path, password, false)?;

    Ok(keys)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use utils_types::traits::PeerIdLike;

    use super::*;

    #[test]
    fn create_identities_from_directory_with_id_files() {
        let path = "./tmp_create";
        let pwd = "password_create";
        match create_identity(path, pwd, &Some(String::from("node1"))) {
            Ok(_) => assert!(true),
            _ => assert!(false),
        }
        remove_json_keystore(path).map_err(|err| println!("{:?}", err)).ok();
    }

    #[test]
    fn read_identities_from_directory_with_id_files() {
        let path = "./tmp_1";
        let pwd = "password";
        let created_id = create_identity(path, pwd, &None).unwrap();

        // created and the read id is identical
        let files = get_files(path, &None);
        let read_id = read_identities(files, &pwd.to_string()).unwrap();
        assert_eq!(read_id.len(), 1);
        assert_eq!(read_id[0].chain_key.1.to_address(), created_id.chain_key.1.to_address());
        assert_eq!(read_id[0].chain_key.1.to_peerid(), created_id.chain_key.1.to_peerid());

        // print the read id
        println!("Debug {:#?}", read_id);
        println!("Display {}", read_id[0]);

        remove_json_keystore(path).map_err(|err| println!("{:?}", err)).ok();
    }

    #[test]
    fn read_identities_from_directory_with_id_files_but_wrong_password() {
        let path = "./tmp_2";
        let pwd = "password";
        let wrong_pwd = "wrong_password";
        create_identity(path, pwd, &None).unwrap();
        let files = get_files(path, &None);
        match read_identities(files, &wrong_pwd.to_string()) {
            Ok(val) => assert_eq!(val.len(), 0),
            _ => assert!(false),
        }
        remove_json_keystore(path).map_err(|err| println!("{:?}", err)).ok();
    }

    #[test]
    fn read_identities_from_directory_without_id_files() {
        let path = "./";
        let files = get_files(path, &None);
        match read_identities(files, &"".to_string()) {
            Ok(val) => assert_eq!(val.len(), 0),
            _ => assert!(false),
        }
    }

    #[test]
    fn read_identities_from_tmp_folder() {
        let path = "./tmp_4";
        let pwd = "local";
        create_identity(path, pwd, &Some(String::from("local-alice"))).unwrap();
        let files = get_files(path, &None);
        match read_identities(files, &pwd.to_string()) {
            Ok(val) => assert_eq!(val.len(), 1),
            _ => assert!(false),
        }
        remove_json_keystore(path).map_err(|err| println!("{:?}", err)).ok();
    }

    #[test]
    fn read_identities_from_tmp_folder_with_prefix() {
        let path = "./tmp_5";
        let pwd = "local";
        create_identity(path, pwd, &Some(String::from("local-alice"))).unwrap();
        let files = get_files(path, &Some("local".to_string()));
        match read_identities(files, &pwd.to_string()) {
            Ok(val) => assert_eq!(val.len(), 1),
            _ => assert!(false),
        }
        remove_json_keystore(path).map_err(|err| println!("{:?}", err)).ok();
    }

    #[test]
    fn read_identities_from_tmp_folder_no_match() {
        let path = "./tmp_6";
        let pwd = "local";
        create_identity(path, pwd, &Some(String::from("local-alice"))).unwrap();
        let files = get_files(path, &Some("npm-".to_string()));
        match read_identities(files, &pwd.to_string()) {
            Ok(val) => assert_eq!(val.len(), 0),
            _ => assert!(false),
        }
        remove_json_keystore(path).map_err(|err| println!("{:?}", err)).ok();
    }

    #[test]
    fn read_identities_from_tmp_folder_with_wrong_prefix() {
        let path = "./tmp_7";
        let pwd = "local";
        create_identity(path, pwd, &Some(String::from("local-alice"))).unwrap();

        let files = get_files(path, &Some("alice".to_string()));
        match read_identities(files, &pwd.to_string()) {
            Ok(val) => assert_eq!(val.len(), 0),
            _ => assert!(false),
        }
        remove_json_keystore(path).map_err(|err| println!("{:?}", err)).ok();
    }

    #[test]
    fn read_complete_identities_from_tmp_folder() {
        let path = "./tmp_8";
        let name = "alice.id";
        let pwd = "e2e-test";

        let weak_crypto_alice_keystore = r#"{"crypto":{"cipher":"aes-128-ctr","cipherparams":{"iv":"9a876992ad22bec8e82fe3452788b800"},"ciphertext":"f08651ab3c237e337f81e8fa6689bb896f35e4eaae34aca504fa30a86ad85f72281ba48a99cb1327435c935b9deb800d60ca2fc46072c5c3b3aafc1861f0a12c","kdf":"scrypt","kdfparams":{"dklen":32,"n":2,"p":1,"r":8,"salt":"82af32aac6a4377ce44877c3ae4f7c5e7b9e409e866a0e75fb3bff86f1fbc66d"},"mac":"524197480216a0d1d1781214de8b76ca08feddf337338765b3cea47f08b319cb"},"id":"c76e561b-bedf-4a9a-87f7-352efd718c9b","version":3}"#;
        let alice_peer_id = "16Uiu2HAmUYnGY3USo8iy13SBFW7m5BMQvC4NETu1fGTdoB86piw7";
        let alice_address = "0x838d3c1d2ff5c576d7b270aaaaaa67e619217aac";

        // create dir if not exist.
        fs::create_dir_all(path).unwrap();
        // save the keystore as file
        fs::write(PathBuf::from(path).join(&name), weak_crypto_alice_keystore.as_bytes()).unwrap();

        let files = get_files(path, &None);
        let val = read_identities(files, &pwd.to_string()).unwrap();
        assert_eq!(val.len(), 1);
        assert_eq!(val[0].chain_key.1.to_peerid_str(), alice_peer_id);
        assert_eq!(val[0].chain_key.1.to_address().to_string(), alice_address);

        remove_json_keystore(path).map_err(|err| println!("{:?}", err)).ok();
    }

    fn remove_json_keystore(path: &str) -> Result<(), HelperErrors> {
        println!("remove_json_keystore {:?}", path);
        match fs::remove_dir_all(path) {
            Ok(_) => Ok(()),
            _ => Err(HelperErrors::UnableToDeleteIdentity),
        }
    }

    fn get_files(identity_directory: &str, identity_prefix: &Option<String>) -> Vec<PathBuf> {
        // early return if failed in reading identity directory
        let directory = fs::read_dir(Path::new(identity_directory)).unwrap();

        // read all the files from the directory that contains
        // 1) "id" in its name
        // 2) the provided idetity_prefix
        let files: Vec<PathBuf> = directory
            .into_iter() // read all the files from the directory
            .filter(|r| r.is_ok()) // Get rid of Err variants for Result<DirEntry>
            .map(|r| r.unwrap().path()) // Read all the files from the given directory
            .filter(|r| r.is_file()) // Filter out folders
            .filter(|r| r.to_str().unwrap().contains("id")) // file name should contain "id"
            .filter(|r| match &identity_prefix {
                Some(identity_prefix) => r
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with(identity_prefix.as_str()),
                _ => true,
            })
            .collect();
        files
    }
}
