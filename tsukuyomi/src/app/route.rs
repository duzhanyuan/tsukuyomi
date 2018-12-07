use {
    super::{
        builder::{Context, Scope},
        uri::{Uri, UriComponent},
    },
    crate::{
        core::{Chain, Never, TryFrom},
        extractor::Extractor,
        fs::NamedFile,
        future::{Future, MaybeFuture},
        generic::{Combine, Func},
        handler::{Handler, MakeHandler, ModifyHandler},
        input::param::{FromPercentEncoded, PercentEncoded},
        output::Responder,
    },
    http::{HttpTryFrom, Method},
    indexmap::{indexset, IndexSet},
    std::{marker::PhantomData, path::Path},
};

/// A set of request methods that a route accepts.
#[derive(Debug, Default)]
pub struct Methods(IndexSet<Method>);

impl TryFrom<Self> for Methods {
    type Error = Never;

    #[inline]
    fn try_from(methods: Self) -> Result<Self, Self::Error> {
        Ok(methods)
    }
}

impl TryFrom<Method> for Methods {
    type Error = Never;

    #[inline]
    fn try_from(method: Method) -> Result<Self, Self::Error> {
        Ok(Methods(indexset! { method }))
    }
}

impl<M> TryFrom<Vec<M>> for Methods
where
    Method: HttpTryFrom<M>,
{
    type Error = http::Error;

    #[inline]
    fn try_from(methods: Vec<M>) -> Result<Self, Self::Error> {
        let methods = methods
            .into_iter()
            .map(Method::try_from)
            .collect::<Result<_, _>>()
            .map_err(Into::into)?;
        Ok(Methods(methods))
    }
}

impl<'a> TryFrom<&'a str> for Methods {
    type Error = failure::Error;

    #[inline]
    fn try_from(methods: &'a str) -> Result<Self, Self::Error> {
        let methods = methods
            .split(',')
            .map(|s| Method::try_from(s.trim()).map_err(Into::into))
            .collect::<http::Result<_>>()?;
        Ok(Methods(methods))
    }
}

mod tags {
    #[derive(Debug)]
    pub struct Completed(());

    #[derive(Debug)]
    pub struct Incomplete(());
}

pub fn root() -> Builder<(), (), self::tags::Incomplete> {
    Builder {
        uri: Uri::root(),
        methods: Methods::default(),
        extractor: (),
        modifier: (),
        _marker: std::marker::PhantomData,
    }
}

pub fn asterisk() -> Builder<(), (), self::tags::Completed> {
    Builder {
        uri: Uri::asterisk(),
        methods: Methods::default(),
        extractor: (),
        modifier: (),
        _marker: std::marker::PhantomData,
    }
}

/// A builder of `Scope` to register a route, which is matched to the requests
/// with a certain path and method(s) and will return its response.
#[derive(Debug)]
pub struct Builder<E: Extractor = (), M = (), T = self::tags::Incomplete> {
    uri: Uri,
    methods: Methods,
    extractor: E,
    modifier: M,
    _marker: PhantomData<T>,
}

impl<E, M> Builder<E, M, self::tags::Incomplete>
where
    E: Extractor,
{
    pub fn segment(mut self, s: impl Into<String>) -> super::Result<Self> {
        self.uri.push(UriComponent::Static(s.into()))?;
        Ok(self)
    }

    pub fn slash(self) -> Builder<E, M, self::tags::Completed> {
        Builder {
            uri: {
                let mut uri = self.uri;
                uri.push(UriComponent::Slash).expect("this is a bug.");
                uri
            },
            methods: self.methods,
            extractor: self.extractor,
            modifier: self.modifier,
            _marker: PhantomData,
        }
    }

    pub fn param<T>(
        self,
        name: impl Into<String>,
    ) -> super::Result<
        Builder<
            impl Extractor<Output = <E::Output as Combine<(T,)>>::Out>,
            M,
            self::tags::Incomplete,
        >,
    >
    where
        T: FromPercentEncoded + Send + 'static,
        E::Output: Combine<(T,)> + Send + 'static,
    {
        let name = name.into();
        Ok(Builder {
            uri: {
                let mut uri = self.uri;
                uri.push(UriComponent::Param(name.clone(), ':'))?;
                uri
            },
            methods: self.methods,
            extractor: Chain::new(
                self.extractor,
                crate::extractor::ready(move |input| match input.params {
                    Some(ref params) => {
                        let s = params.name(&name).ok_or_else(|| {
                            crate::error::internal_server_error("invalid paramter name")
                        })?;
                        T::from_percent_encoded(unsafe { PercentEncoded::new_unchecked(s) })
                            .map_err(Into::into)
                    }
                    None => Err(crate::error::internal_server_error("missing Params")),
                }),
            ),
            modifier: self.modifier,
            _marker: PhantomData,
        })
    }

    pub fn catch_all<T>(
        self,
        name: impl Into<String>,
    ) -> super::Result<
        Builder<
            impl Extractor<Output = <E::Output as Combine<(T,)>>::Out>,
            M,
            self::tags::Completed,
        >,
    >
    where
        T: FromPercentEncoded + Send + 'static,
        E::Output: Combine<(T,)> + Send + 'static,
    {
        let name = name.into();
        Ok(Builder {
            uri: {
                let mut uri = self.uri;
                uri.push(UriComponent::Param(name.clone(), '*'))?;
                uri
            },
            methods: self.methods,
            extractor: Chain::new(
                self.extractor,
                crate::extractor::ready(|input| match input.params {
                    Some(ref params) => {
                        let s = params.catch_all().ok_or_else(|| {
                            crate::error::internal_server_error(
                                "the catch-all parameter is not available",
                            )
                        })?;
                        T::from_percent_encoded(unsafe { PercentEncoded::new_unchecked(s) })
                            .map_err(Into::into)
                    }
                    None => Err(crate::error::internal_server_error("missing Params")),
                }),
            ),
            modifier: self.modifier,
            _marker: PhantomData,
        })
    }
}

impl<E, M, T> Builder<E, M, T>
where
    E: Extractor,
{
    /// Sets the HTTP methods that this route accepts.
    pub fn methods<M2>(self, methods: M2) -> super::Result<Self>
    where
        Methods: TryFrom<M2>,
    {
        Ok(Builder {
            methods: Methods::try_from(methods).map_err(Into::into)?,
            ..self
        })
    }

    /// Appends an `Extractor` to this builder.
    pub fn extract<E2>(self, other: E2) -> Builder<Chain<E, E2>, M, T>
    where
        E2: Extractor,
        E::Output: Combine<E2::Output> + Send + 'static,
        E2::Output: Send + 'static,
    {
        Builder {
            extractor: Chain::new(self.extractor, other),
            modifier: self.modifier,
            uri: self.uri,
            methods: self.methods,
            _marker: PhantomData,
        }
    }

    /// Appends a `ModifyHandler` to this builder.
    pub fn modify<M2>(self, modifier: M2) -> Builder<E, Chain<M, M2>, T> {
        Builder {
            extractor: self.extractor,
            modifier: Chain::new(self.modifier, modifier),
            uri: self.uri,
            methods: self.methods,
            _marker: PhantomData,
        }
    }

    pub fn finish<F>(self, make_handler: F) -> Route<F::Handler, M>
    where
        F: MakeHandler<E>,
    {
        Route {
            uri: self.uri,
            methods: self.methods,
            handler: make_handler.make_handler(self.extractor),
            modifier: self.modifier,
        }
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The provided function always succeeds and immediately returns a value.
    pub fn reply<F>(self, f: F) -> Route<impl Handler<Output = F::Out>, M>
    where
        F: Func<E::Output> + Clone + Send + 'static,
    {
        self.finish(|extractor: E| {
            crate::handler::raw(move |input| match extractor.extract(input) {
                MaybeFuture::Ready(result) => {
                    MaybeFuture::Ready(result.map(|args| f.call(args)).map_err(Into::into))
                }
                MaybeFuture::Future(mut future) => MaybeFuture::Future({
                    let f = f.clone();
                    crate::future::poll_fn(move |cx| {
                        let args = futures01::try_ready!(future.poll_ready(cx).map_err(Into::into));
                        Ok(f.call(args).into())
                    })
                }),
            })
        })
    }

    /// Creates an instance of `Route` with the current configuration and the specified function.
    ///
    /// The result of provided function is returned by `Future`.
    pub fn call<F, R>(self, f: F) -> Route<impl Handler<Output = R::Output>, M>
    where
        F: Func<E::Output, Out = R> + Clone + Send + 'static,
        R: Future + Send + 'static,
    {
        #[allow(missing_debug_implementations)]
        enum State<F1, F2, F> {
            First(F1, F),
            Second(F2),
        }

        self.finish(|extractor: E| {
            crate::handler::raw(move |input| {
                let mut state = match extractor.extract(input) {
                    MaybeFuture::Ready(Ok(args)) => State::Second(f.call(args)),
                    MaybeFuture::Ready(Err(err)) => return MaybeFuture::err(err.into()),
                    MaybeFuture::Future(future) => State::First(future, f.clone()),
                };
                MaybeFuture::Future(crate::future::poll_fn(move |cx| loop {
                    state = match state {
                        State::First(ref mut f1, ref f) => {
                            let args = futures01::try_ready!(f1.poll_ready(cx).map_err(Into::into));
                            State::Second(f.call(args))
                        }
                        State::Second(ref mut f2) => return f2.poll_ready(cx).map_err(Into::into),
                    }
                }))
            })
        })
    }
}

impl<M, T> Builder<(), M, T> {
    /// Builds a `Route` that uses the specified `Handler` directly.
    pub fn raw<H>(self, handler: H) -> Route<H, M>
    where
        H: Handler,
    {
        self.finish(|_: ()| handler)
    }
}

impl<E, M, T> Builder<E, M, T>
where
    E: Extractor<Output = ()>,
{
    /// Creates a `Route` that just replies with the specified `Responder`.
    pub fn say<R>(self, output: R) -> Route<impl Handler<Output = R>, M>
    where
        R: Clone + Send + 'static,
    {
        self.reply(move || output.clone())
    }

    /// Creates a `Route` that sends the contents of file located at the specified path.
    pub fn send_file(
        self,
        path: impl AsRef<Path>,
        config: Option<crate::fs::OpenConfig>,
    ) -> Route<impl Handler<Output = NamedFile>, M> {
        let path = crate::fs::ArcPath::from(path.as_ref().to_path_buf());

        self.call(move || {
            crate::future::Compat01::from(match config {
                Some(ref config) => NamedFile::open_with_config(path.clone(), config.clone()),
                None => NamedFile::open(path.clone()),
            })
        })
    }
}

#[derive(Debug)]
pub struct Route<H, M> {
    methods: Methods,
    uri: Uri,
    handler: H,
    modifier: M,
}

impl<H, M1, M2> Scope<M1> for Route<H, M2>
where
    H: Handler,
    M2: ModifyHandler<H>,
    M1: ModifyHandler<M2::Handler>,
    M1::Output: Responder,
    M1::Handler: Send + Sync + 'static,
{
    type Error = super::Error;

    fn configure(self, cx: &mut Context<'_, M1>) -> Result<(), Self::Error> {
        cx.add_endpoint(
            &self.uri,
            self.methods.0,
            self.modifier.modify(self.handler),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_methods_try_from() {
        assert_eq!(
            Methods::try_from(Methods(indexset! { Method::GET }))
                .unwrap()
                .0,
            indexset! { Method::GET }
        );
        assert_eq!(
            Methods::try_from(Method::GET).unwrap().0,
            indexset! { Method::GET }
        );
        assert_eq!(
            Methods::try_from(vec![Method::GET, Method::POST])
                .unwrap()
                .0,
            indexset! { Method::GET, Method::POST }
        );
        assert_eq!(
            Methods::try_from("GET").unwrap().0,
            indexset! { Method::GET }
        );
        assert_eq!(
            Methods::try_from("GET, POST").unwrap().0,
            indexset! { Method::GET , Method::POST }
        );
        assert!(Methods::try_from("").is_err());
    }
}
