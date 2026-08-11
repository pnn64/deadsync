use super::StreamBuilder;
use crate::prelude::{Delimiter, Result};

/// A builder for functions.
pub struct FnBuilder<'a, P> {
    parent: &'a mut P,
    name: String,

    generics: Vec<(String, Vec<String>)>,
    self_arg: FnSelfArg,
    args: Vec<(String, String)>,
    return_type: Option<String>,
}

impl<'a, P: FnParent> FnBuilder<'a, P> {
    pub(super) fn new(parent: &'a mut P, name: impl Into<String>) -> Self {
        Self {
            parent,
            name: name.into(),
            generics: Vec::new(),
            self_arg: FnSelfArg::None,
            args: Vec::new(),
            return_type: None,
        }
    }

    /// Add a generic parameter. Keep in mind that will *not* work for lifetimes.
    ///
    /// `dependencies` are the dependencies of the parameter.
    ///
    /// ```
    /// # use virtue::prelude::Generator;
    /// # let mut generator = Generator::with_name("Foo");
    /// generator
    ///     .r#impl()
    ///     .generate_fn("foo") // fn foo()
    ///     .with_generic("D") // fn foo<D>()
    ///     .with_generic_deps("E", ["Encodable"]) // fn foo<D, E: Encodable>();
    /// # .body(|_| Ok(())).unwrap();
    /// # generator.assert_eq("impl Foo { fn foo < D , E : Encodable > () { } }");
    /// ```
    #[must_use]
    pub fn with_generic_deps<DEP, I>(mut self, name: impl Into<String>, dependencies: DEP) -> Self
    where
        DEP: IntoIterator<Item = I>,
        I: Into<String>,
    {
        self.generics.push((
            name.into(),
            dependencies.into_iter().map(Into::into).collect(),
        ));
        self
    }

    /// Set the value for `self`. See [FnSelfArg] for more information.
    ///
    /// ```
    /// # use virtue::prelude::{Generator, FnSelfArg};
    /// # let mut generator = Generator::with_name("Foo");
    /// generator
    ///     .r#impl()
    ///     .generate_fn("foo") // fn foo()
    ///     .with_self_arg(FnSelfArg::RefSelf) // fn foo(&self)
    /// # .body(|_| Ok(())).unwrap();
    /// # generator.assert_eq("impl Foo { fn foo (& self ,) { } }");
    /// ```
    #[must_use]
    pub fn with_self_arg(mut self, self_arg: FnSelfArg) -> Self {
        self.self_arg = self_arg;
        self
    }

    /// Add an argument with a `name` and a `ty`.
    ///
    /// ```
    /// # use virtue::prelude::Generator;
    /// # let mut generator = Generator::with_name("Foo");
    /// generator
    ///     .r#impl()
    ///     .generate_fn("foo") // fn foo()
    ///     .with_arg("a", "u32") // fn foo(a: u32)
    ///     .with_arg("b", "u32") // fn foo(a: u32, b: u32)
    /// # .body(|_| Ok(())).unwrap();
    /// # generator.assert_eq("impl Foo { fn foo (a : u32 , b : u32) { } }");
    /// ```
    #[must_use]
    pub fn with_arg(mut self, name: impl Into<String>, ty: impl Into<String>) -> Self {
        self.args.push((name.into(), ty.into()));
        self
    }

    /// Set the return type for the function. By default the function will have no return type.
    ///
    /// ```
    /// # use virtue::prelude::Generator;
    /// # let mut generator = Generator::with_name("Foo");
    /// generator
    ///     .r#impl()
    ///     .generate_fn("foo") // fn foo()
    ///     .with_return_type("u32") // fn foo() -> u32
    /// # .body(|_| Ok(())).unwrap();
    /// # generator.assert_eq("impl Foo { fn foo () ->u32 { } }");
    /// ```
    #[must_use]
    pub fn with_return_type(mut self, ret_type: impl Into<String>) -> Self {
        self.return_type = Some(ret_type.into());
        self
    }

    /// Complete the function definition. This function takes a callback that will form the body of the function.
    ///
    /// ```
    /// # use virtue::prelude::Generator;
    /// # let mut generator = Generator::with_name("Foo");
    /// generator
    ///     .r#impl()
    ///     .generate_fn("foo") // fn foo()
    ///     .body(|b| {
    ///         b.push_parsed("println!(\"hello world\");")?;
    ///         Ok(())
    ///     })
    ///     .unwrap();
    /// // fn foo() {
    /// //     println!("Hello world");
    /// // }
    /// # generator.assert_eq("impl Foo { fn foo () { println ! (\"hello world\") ; } }");
    /// ```
    pub fn body(
        self,
        body_builder: impl FnOnce(&mut StreamBuilder) -> crate::Result,
    ) -> crate::Result {
        let FnBuilder {
            parent,
            name,
            generics,
            self_arg,
            args,
            return_type,
        } = self;

        let mut builder = StreamBuilder::new();
        builder.ident_str("fn");
        builder.ident_str(name);

        if !generics.is_empty() {
            builder.punct('<');
            for (generic_index, (generic, dependencies)) in generics.into_iter().enumerate() {
                if generic_index != 0 {
                    builder.punct(',');
                }
                builder.ident_str(&generic);
                if !dependencies.is_empty() {
                    for (idx, dependency) in dependencies.into_iter().enumerate() {
                        builder.punct(if idx == 0 { ':' } else { '+' });
                        builder.push_parsed(&dependency)?;
                    }
                }
            }
            builder.punct('>');
        }

        // Arguments; `(&self, foo: &Bar)`
        builder.group(Delimiter::Parenthesis, |arg_stream| {
            if let Some(self_arg) = self_arg.into_token_tree() {
                arg_stream.append(self_arg);
                arg_stream.punct(',');
            }
            for (idx, (arg_name, arg_ty)) in args.into_iter().enumerate() {
                if idx != 0 {
                    arg_stream.punct(',');
                }
                arg_stream.push_parsed(&arg_name)?;
                arg_stream.punct(':');
                arg_stream.push_parsed(&arg_ty)?;
            }
            Ok(())
        })?;

        // Return type: `-> ResultType`
        if let Some(return_type) = return_type {
            builder.puncts("->");
            builder.push_parsed(&return_type)?;
        }

        let mut body_stream = StreamBuilder::new();
        body_builder(&mut body_stream)?;

        parent.append(builder, body_stream)
    }
}

pub trait FnParent {
    fn append(&mut self, fn_definition: StreamBuilder, fn_body: StreamBuilder) -> Result;
}

/// The `self` argument used by bincode's generated functions.
pub enum FnSelfArg {
    None,
    RefSelf,
}

impl FnSelfArg {
    fn into_token_tree(self) -> Option<StreamBuilder> {
        let mut builder = StreamBuilder::new();
        match self {
            Self::None => return None,
            Self::RefSelf => {
                builder.punct('&');
                builder.ident_str("self");
            }
        }
        Some(builder)
    }
}
