use super::{generate_item::FnParent, FnBuilder, Parent, StreamBuilder, StringOrIdent};
use crate::{
    parse::{GenericConstraints, Generics},
    prelude::{Delimiter, Result},
};

#[must_use]
/// A helper struct for implementing a trait for a given struct or enum.
pub struct ImplFor<'a, P: Parent> {
    generator: &'a mut P,
    type_name: StringOrIdent,
    trait_name: Option<StringOrIdent>,
    lifetimes: Option<Vec<String>>,
    trait_generics: Option<Vec<String>>,
    impl_generics: Vec<String>,
    custom_generic_constraints: Option<GenericConstraints>,
    fns: Vec<(StreamBuilder, StreamBuilder)>,
}

impl<'a, P: Parent> ImplFor<'a, P> {
    pub(super) fn new(
        generator: &'a mut P,
        type_name: StringOrIdent,
        trait_name: Option<StringOrIdent>,
    ) -> Self {
        Self {
            generator,
            trait_name,
            type_name,
            lifetimes: None,
            trait_generics: None,
            impl_generics: vec![],
            custom_generic_constraints: None,
            fns: Vec::new(),
        }
    }

    /// Internal helper function to set lifetimes
    pub(crate) fn with_lifetimes<ITER>(mut self, lifetimes: ITER) -> Self
    where
        ITER: IntoIterator,
        ITER::Item: Into<String>,
    {
        self.lifetimes = Some(lifetimes.into_iter().map(Into::into).collect());
        self
    }

    /// Add generic parameters to the trait implementation.
    ///```
    /// # use virtue::prelude::Generator;
    /// # let mut generator = Generator::with_name("Bar");
    /// generator.impl_for("Foo")
    ///          .with_trait_generics(["Baz"]);
    /// # generator.assert_eq("impl Foo < Baz > for Bar { }");
    /// # Ok::<_, virtue::Error>(())
    /// ```
    ///
    /// Generates:
    /// ```ignore
    /// impl Foo for <struct or enum> {
    ///     const BAR: u8 = 5;
    /// }
    /// ```
    pub fn with_trait_generics<ITER>(mut self, generics: ITER) -> Self
    where
        ITER: IntoIterator,
        ITER::Item: Into<String>,
    {
        self.trait_generics = Some(generics.into_iter().map(Into::into).collect());
        self
    }

    /// Add generic parameters to the impl block.
    ///```
    /// # use virtue::prelude::Generator;
    /// # let mut generator = Generator::with_name("Bar");
    /// generator.impl_for("Foo")
    ///          .with_impl_generics(["Baz"]);
    /// # generator.assert_eq("impl < Baz > Foo for Bar { }");
    /// # Ok::<_, virtue::Error>(())
    /// ```
    ///
    /// Generates:
    /// ```ignore
    /// impl<Baz> Foo for Bar { }
    /// ```
    pub fn with_impl_generics<ITER>(mut self, generics: ITER) -> Self
    where
        ITER: IntoIterator,
        ITER::Item: Into<String>,
    {
        self.impl_generics = generics.into_iter().map(Into::into).collect();
        self
    }

    /// Add a function to the trait implementation.
    ///
    /// `generator.impl_for("Foo").generate_fn("bar")` results in code like:
    ///
    /// ```ignore
    /// impl Foo for <struct or enum> {
    ///     fn bar() {}
    /// }
    /// ```
    ///
    /// See [`FnBuilder`] for more options, as well as information on how to fill the function body.
    pub fn generate_fn(&mut self, name: impl Into<String>) -> FnBuilder<'_, ImplFor<'a, P>> {
        FnBuilder::new(self, name)
    }

    ///
    /// Modify the generic constraints of a type.
    /// This can be used to add additional type constraints to your implementation.
    ///
    /// ```ignore
    /// // Your derive:
    /// #[derive(YourTrait)]
    /// pub struct Foo<B> {
    ///     ...
    /// }
    ///
    /// // With this code:
    /// generator
    ///     .impl_for("YourTrait")
    ///     .modify_generic_constraints(|generics, constraints| {
    ///         for g in generics.iter_generics() {
    ///             constraints.push_generic(g, "YourTrait");
    ///         }
    ///     })
    ///
    /// // will generate:
    /// impl<B> YourTrait for Foo<B>
    ///     where B: YourTrait // <-
    /// {
    /// }
    /// ```
    ///
    pub fn modify_generic_constraints<CB>(&mut self, cb: CB) -> Result<&mut Self>
    where
        CB: FnOnce(&Generics, &mut GenericConstraints) -> Result,
    {
        if let Some(generics) = self.generator.generics() {
            let constraints = self.custom_generic_constraints.get_or_insert_with(|| {
                self.generator
                    .generic_constraints()
                    .cloned()
                    .unwrap_or_default()
            });
            cb(generics, constraints)?;
        }
        Ok(self)
    }
}

impl<'a, P: Parent> FnParent for ImplFor<'a, P> {
    fn append(&mut self, fn_definition: StreamBuilder, fn_body: StreamBuilder) -> Result {
        self.fns.push((fn_definition, fn_body));
        Ok(())
    }
}

impl<P: Parent> Drop for ImplFor<'_, P> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let mut builder = StreamBuilder::new();
        self.generate_impl_definition(&mut builder);

        builder
            .group(Delimiter::Brace, |builder| {
                for (fn_def, fn_body) in std::mem::take(&mut self.fns) {
                    builder.append(fn_def);
                    builder
                        .group(Delimiter::Brace, |body| {
                            *body = fn_body;
                            Ok(())
                        })
                        .unwrap();
                }
                Ok(())
            })
            .unwrap();

        self.generator.append(builder);
    }
}

impl<P: Parent> ImplFor<'_, P> {
    fn generate_impl_definition(&mut self, builder: &mut StreamBuilder) {
        builder.ident_str("impl");

        let impl_generics = self.impl_generics.as_slice();
        if let Some(lifetimes) = &self.lifetimes {
            if let Some(generics) = self.generator.generics() {
                builder.append(generics.impl_generics_with_additional(lifetimes, impl_generics));
            } else {
                append_lifetimes_and_generics(builder, lifetimes, impl_generics);
            }
        } else if let Some(generics) = self.generator.generics() {
            builder.append(generics.impl_generics_with_additional(&[], impl_generics));
        } else if !impl_generics.is_empty() {
            append_lifetimes_and_generics(builder, &[], impl_generics)
        }
        if let Some(t) = &self.trait_name {
            builder.push_parsed(t.to_string()).unwrap();

            let lifetimes = self.lifetimes.as_deref().unwrap_or_default();
            let generics = self.trait_generics.as_deref().unwrap_or_default();
            append_lifetimes_and_generics(builder, lifetimes, generics);
            builder.ident_str("for");
        }
        builder.push_parsed(self.type_name.to_string()).unwrap();
        if let Some(generics) = &self.generator.generics() {
            builder.append(generics.type_generics());
        }
        if let Some(generic_constraints) = self.custom_generic_constraints.take() {
            builder.append(generic_constraints.where_clause());
        } else if let Some(generic_constraints) = &self.generator.generic_constraints() {
            builder.append(generic_constraints.where_clause());
        }
    }
}

fn append_lifetimes_and_generics(
    builder: &mut StreamBuilder,
    lifetimes: &[String],
    generics: &[String],
) {
    if lifetimes.is_empty() && generics.is_empty() {
        return;
    }

    builder.punct('<');

    for (idx, lt) in lifetimes.iter().enumerate() {
        if idx > 0 {
            builder.punct(',');
        }
        builder.lifetime_str(lt);
    }

    for (idx, gen) in generics.iter().enumerate() {
        if idx > 0 || !lifetimes.is_empty() {
            builder.punct(',');
        }
        builder.push_parsed(gen).unwrap();
    }

    builder.punct('>');
}
