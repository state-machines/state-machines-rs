use super::*;
use quote::quote;
#[test]
fn rejects_invalid_region_graphs() {
    for input in [
        quote! { name: Test, regions: { a: A } },
        quote! { name: Test, regions: { a: A, a: B } },
        quote! { name: Test, regions: { a: A, b: B }, events { go { routes: {} } } },
        quote! { name: Test, regions: { a: A, b: B }, events { go { routes: { c: Event::Go } } } },
        quote! { name: Test, regions: { a: A, b: B }, events { go { routes: { a: Event::Go, a: Event::Go } } } },
    ] {
        assert!(syn::parse2::<Regions>(input).is_err());
    }
}
