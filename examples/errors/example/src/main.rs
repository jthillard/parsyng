use errors_macros::{Getters, hex_color};

#[derive(Getters)]
struct Person<T> {
    name: String,
    age: u8,
    extra: T,
}

// Uncomment any of the following to see the errors returned by the macros
// (see the README for the compiler output).

// const MISSING_HASH: (u8, u8, u8) = hex_color!("ff8800");
// const TOO_SHORT: (u8, u8, u8) = hex_color!("#ff88");
// const NOT_HEX: (u8, u8, u8) = hex_color!("#gg0000");
// const NOT_A_STRING: (u8, u8, u8) = hex_color!(42);

// #[derive(Getters)]
// enum NotAStruct {
//     A,
// }

// #[derive(Getters)]
// struct Tuple(u8);

// #[derive(Getters)]
// struct BadNames {
//     get_x: u8,
//     y: u8,
//     get_z: u8,
// }

fn main() {
    const ORANGE: (u8, u8, u8) = hex_color!("#ff8800");
    assert_eq!(ORANGE, (255, 136, 0));
    assert_eq!(hex_color!("#000000"), (0, 0, 0));

    let person = Person {
        name: "Ferris".to_owned(),
        age: 9,
        extra: [1, 2],
    };
    assert_eq!(person.get_name(), "Ferris");
    assert_eq!(*person.get_age(), 9);
    assert_eq!(person.get_extra(), &[1, 2]);

    println!(
        "orange = {ORANGE:?}, {} is {}",
        person.get_name(),
        person.get_age()
    );
}
