/// This macros is needed to create function that will initialize a mock object and a locker for $ type
/// because mockall doesn't support multithreading.
/// Example:
/// ```
/// struct Example {}
/// #[cfg_attr(test, automock)]
/// impl Example {
///     fn foo(&self) -> String {
///         "test".into()
///     }
/// }
///
/// mmb_utils::impl_mock_initializer!(MockExample);
///
/// #[cfg(test)]
/// mod test {
///     use mockall_double::double;
///
///     #[double]
///     use super::Example;
///
///     #[test]
///     fn test_example() {
///         let (mut example_mock, _example_locker) = Example::init_mock();
///         // here you can use example_mock while example_locker is alive
///
///         example_mock.expect_foo().returning(|| "hello".into());
///
///         assert_eq!(example_mock.foo(), "hello");
///     }
/// }
/// ```
#[macro_export]
macro_rules! impl_mock_initializer {
    ($type: ident) => {
        #[cfg(test)]
        #[allow(unused_qualifications)]
        impl $type {
            pub fn init_mock() -> ($type, parking_lot::ReentrantMutexGuard<'static, ()>) {
                let locker = MOCK_MUTEX.lock();
                ($type::default(), locker)
            }
        }
    };
}
