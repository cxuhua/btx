
custom_derive! {
    #[derive(Debug, EnumFromStr)]
    enum Foo {
        Bar,
        Baz,
        Bat,
        Quux
    }
}

#[test]
fn test_enum_parse() {
    let variable: Foo = "Quux".parse().unwrap();
    let eq = if let Foo::Bar = variable {
        true
    } else {
        false
    };
    println!("{}",eq);
}


#[test]
fn test_for() {
    let result = (1..=5).fold(0, |acc, x| acc + x * x);
    println!("result = {}", result);

  
}

use secp256k1::rand::rngs::OsRng;
use secp256k1::{PublicKey, Secp256k1, SecretKey};

#[test]
fn test_secp256k1() {
    let secp = Secp256k1::new();
    let mut rng = OsRng::new().unwrap();
    // First option:
    let (seckey, pubkey) = secp.generate_keypair(&mut rng);

    assert_eq!(pubkey, PublicKey::from_secret_key(&secp, &seckey));

    // Second option:
    let seckey = SecretKey::new(&mut rng);
    println!("{}",seckey);
    let pubkey = PublicKey::from_secret_key(&secp, &seckey);
    println!("{:?}",pubkey.serialize());
}