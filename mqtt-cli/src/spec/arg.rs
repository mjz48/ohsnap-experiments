/// Flag argument specification. Flags can come with no argument, optional
/// argument, or required argument.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Arg {
    #[default]
    None,
    Optional,
    Required,
}
