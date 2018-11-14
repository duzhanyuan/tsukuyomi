use super::*;

#[derive(Debug)]
pub struct Or<L, R> {
    pub(super) left: L,
    pub(super) right: R,
}

impl<L, R> Extractor for Or<L, R>
where
    L: Extractor,
    R: Extractor<Output = L::Output>,
{
    type Output = L::Output;
    type Error = Error;
    type Future = OrFuture<L::Future, R::Future>;

    fn extract(&self, input: &mut Input<'_>) -> Extract<Self> {
        let left_status = match self.left.extract(input) {
            Ok(status) => status,
            Err(..) => {
                return self
                    .right
                    .extract(input)
                    .map(|status| status.map_pending(OrFuture::Right))
                    .map_err(Into::into)
            }
        };

        let left = match left_status {
            status @ ExtractStatus::Ready(..) | status @ ExtractStatus::Canceled(..) => {
                return Ok(status.map_pending(|_| unreachable!()));
            }
            ExtractStatus::Pending(left) => left,
        };

        match self.right.extract(input) {
            Ok(status) => Ok(status.map_pending(|right| {
                OrFuture::Both(
                    left.map_err(Into::into as fn(L::Error) -> Error)
                        .select(right.map_err(Into::into as fn(R::Error) -> Error)),
                )
            })),
            Err(..) => Ok(ExtractStatus::Pending(OrFuture::Left(left))),
        }
    }
}

#[doc(hidden)]
#[allow(missing_debug_implementations)]
#[cfg_attr(feature = "cargo-clippy", allow(stutter, type_complexity))]
pub enum OrFuture<L, R>
where
    L: Future,
    R: Future<Item = L::Item>,
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    Left(L),
    Right(R),
    Both(
        future::Select<
            future::MapErr<L, fn(L::Error) -> Error>,
            future::MapErr<R, fn(R::Error) -> Error>,
        >,
    ),
}

impl<L, R> Future for OrFuture<L, R>
where
    L: Future,
    R: Future<Item = L::Item>,
    L::Error: Into<Error>,
    R::Error: Into<Error>,
{
    type Item = L::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self {
            OrFuture::Both(ref mut future) => future
                .poll()
                .map(|x| x.map(|(out, _next)| out))
                .map_err(|(err, _next)| err),
            OrFuture::Left(ref mut left) => left.poll().map_err(Into::into),
            OrFuture::Right(ref mut right) => right.poll().map_err(Into::into),
        }
    }
}