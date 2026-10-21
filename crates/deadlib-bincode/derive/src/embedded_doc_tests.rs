//! Executable examples for the embedded, private token-building API.

fn generator(source: &str) -> crate::prelude::Generator {
    crate::prelude::Parse::new(source.parse().unwrap())
        .unwrap()
        .into_generator()
        .0
}

fn assert_generated(generator: crate::prelude::Generator, expected: &str) {
    let actual = generator.finish().unwrap().to_string();
    assert_eq!(actual, expected);
}

#[test]
fn append_generic_constraint() {
    let mut generator = generator("struct Foo<T, U> where T: First {}");
    generator
        .impl_for("Example")
        .modify_generic_constraints(|generics, constraints| {
            let second = generics
                .iter_generics()
                .find(|generic| generic.ident == "U")
                .unwrap();
            constraints.push_constraint(second, "Second")
        })
        .unwrap();
    assert_generated(
        generator,
        "impl < T , U > Example for Foo < T , U > where T : First , U : Second { }",
    );
}

// Preserves the former private Virtue example in src/generate/generate_item.rs:31.
#[test]
fn function_generics() {
    let mut generator = generator("struct Foo;");
    generator
        .impl_for("Example")
        .generate_fn("foo") // fn foo()
        .with_generic_deps("D", [] as [&str; 0]) // fn foo<D>()
        .with_generic_deps("E", ["Encodable"]) // fn foo<D, E: Encodable>();
        .body(|_| Ok(()))
        .unwrap();
    assert_generated(
        generator,
        "impl Example for Foo { fn foo < D , E : Encodable > () { } }",
    );
}

// Preserves the former private Virtue example in src/generate/generate_item.rs:57.
#[test]
fn function_self_arg() {
    use virtue::prelude::FnSelfArg;
    let mut generator = generator("struct Foo;");
    generator
        .impl_for("Example")
        .generate_fn("foo") // fn foo()
        .with_self_arg(FnSelfArg::RefSelf) // fn foo(&self)
        .body(|_| Ok(()))
        .unwrap();
    assert_generated(generator, "impl Example for Foo { fn foo (& self ,) { } }");
}

// Preserves the former private Virtue example in src/generate/generate_item.rs:75.
#[test]
fn function_arguments() {
    let mut generator = generator("struct Foo;");
    generator
        .impl_for("Example")
        .generate_fn("foo") // fn foo()
        .with_arg("a", "u32") // fn foo(a: u32)
        .with_arg("b", "u32") // fn foo(a: u32, b: u32)
        .body(|_| Ok(()))
        .unwrap();
    assert_generated(
        generator,
        "impl Example for Foo { fn foo (a : u32 , b : u32) { } }",
    );
}

// Preserves the former private Virtue example in src/generate/generate_item.rs:94.
#[test]
fn function_return_type() {
    let mut generator = generator("struct Foo;");
    generator
        .impl_for("Example")
        .generate_fn("foo") // fn foo()
        .with_return_type("u32") // fn foo() -> u32
        .body(|_| Ok(()))
        .unwrap();
    assert_generated(generator, "impl Example for Foo { fn foo () ->u32 { } }");
}

// Preserves the former private Virtue example in src/generate/generate_item.rs:112.
#[test]
fn function_body() {
    let mut generator = generator("struct Foo;");
    generator
        .impl_for("Example")
        .generate_fn("foo") // fn foo()
        .body(|b| {
            b.push_parsed("println!(\"hello world\");")?;
            Ok(())
        })
        .unwrap();
    // fn foo() {
    //     println!("Hello world");
    // }
    assert_generated(
        generator,
        "impl Example for Foo { fn foo () { println ! (\"hello world\") ; } }",
    );
}

// Preserves the former private Virtue example in src/generate/generator.rs:54.
#[test]
fn impl_with_lifetimes() {
    let mut generator = generator("struct Bar;");
    generator.impl_for_with_lifetimes("Foo", ["a", "b"]);

    // will output:
    // impl<'a, 'b> Foo<'a, 'b> for StructOrEnum { }
    assert_generated(generator, "impl < 'a , 'b > Foo < 'a , 'b > for Bar { }");
}

// Preserves the former private Virtue example in src/generate/generator.rs:66.
#[test]
fn dependent_lifetimes() {
    let mut generator = generator("struct Bar<'a>;");
    // given a derive on `struct<'a> Bar<'a>`
    generator
        .impl_for_with_lifetimes("Foo", ["b"])
        .modify_generic_constraints(|_, constraints| constraints.push_parsed_constraint("'b: 'a"))
        .unwrap();

    // will output:
    // impl<'a, 'b> Foo<'b> for Bar<'a> where 'b: 'a { }
    assert_generated(
        generator,
        "impl < 'b , 'a > Foo < 'b > for Bar < 'a > where 'b : 'a { }",
    );
}

// Preserves the former private Virtue example in src/generate/impl_for.rs:49.
#[test]
fn trait_generics() -> crate::Result {
    let mut generator = generator("struct Bar;");
    generator.impl_for("Foo").with_trait_generics(["Baz"]);
    assert_generated(generator, "impl Foo < Baz > for Bar { }");
    Ok::<_, virtue::Error>(())
}

// Preserves the former private Virtue example in src/generate/impl_for.rs:74.
#[test]
fn impl_generics() -> crate::Result {
    let mut generator = generator("struct Bar;");
    generator.impl_for("Foo").with_impl_generics(["Baz"]);
    assert_generated(generator, "impl < Baz > Foo for Bar { }");
    Ok::<_, virtue::Error>(())
}

// Preserves the former private Virtue example in src/parse/mod.rs:24.
#[test]
fn parse_derive_input() {
    use crate::prelude::{Body, Parse};
    let parse = Parse::new("struct YourType;".parse().unwrap()).unwrap();
    let (_, _, body) = parse.into_generator();
    assert!(matches!(body, Body::Struct(_)));
}

// Preserves the former private Virtue example in src/utils.rs:13.
#[test]
fn tagged_attributes() {
    use crate::prelude::{Group, TokenStream, TokenTree};
    use std::str::FromStr;
    fn parse_token_stream_group(input: &'static str) -> Group {
        let token_stream: TokenStream = proc_macro2::TokenStream::from_str(input).unwrap().into();
        let mut iter = token_stream.into_iter();
        let Some(TokenTree::Punct(_)) = iter.next() else {
            panic!()
        };
        let Some(TokenTree::Group(group)) = iter.next() else {
            panic!()
        };
        group
    }
    use virtue::utils::{parse_tagged_attribute, ParsedAttribute};

    // The attribute being parsed
    let group: Group = parse_token_stream_group("#[prefix(result, foo = \"bar\")]");

    let attributes = parse_tagged_attribute(&group, "prefix").unwrap().unwrap();
    let mut iter = attributes.into_iter();

    // The stream will contain the contents of the `prefix(...)`
    match iter.next() {
        Some(ParsedAttribute::Tag(i)) => {
            assert_eq!(i.to_string(), String::from("result"));
        }
        x => panic!("Unexpected attribute: {:?}", x),
    }
    match iter.next() {
        Some(ParsedAttribute::Property(key, val)) => {
            assert_eq!(key.to_string(), String::from("foo"));
            assert_eq!(val.to_string(), String::from("\"bar\""));
        }
        x => panic!("Unexpected attribute: {:?}", x),
    }
}
