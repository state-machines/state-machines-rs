use super::*;
use proc_macro2::Span;

#[test]
fn test_to_snake_case() {
    assert_eq!(to_snake_case("Trip"), "trip");
    assert_eq!(to_snake_case("trip"), "trip");
    assert_eq!(to_snake_case("EnterHalfOpen"), "enter_half_open");
    assert_eq!(to_snake_case("HTTPRequest"), "http_request");
    assert_eq!(to_snake_case("XMLParser"), "xml_parser");
    assert_eq!(to_snake_case("IOError"), "io_error");
    assert_eq!(to_snake_case("parseXML"), "parse_xml");
    assert_eq!(to_snake_case("sendHTTPRequest"), "send_http_request");
    assert_eq!(to_snake_case("A"), "a");
    assert_eq!(to_snake_case("AB"), "ab");
    assert_eq!(to_snake_case("ABC"), "abc");
    assert_eq!(to_snake_case("ABCDef"), "abc_def");
    assert_eq!(to_snake_case("snake_case"), "snake_case");
    assert_eq!(to_snake_case("SCREAMING_SNAKE"), "screaming_snake");
}

#[test]
fn test_to_snake_case_ident() {
    let pascal = Ident::new("EnterHalfOpen", Span::call_site());
    let snake = to_snake_case_ident(&pascal);
    assert_eq!(snake.to_string(), "enter_half_open");
}

#[test]
fn test_to_pascal_case() {
    assert_eq!(to_pascal_case("next"), "Next");
    assert_eq!(to_pascal_case("enter_half_open"), "EnterHalfOpen");
    assert_eq!(to_pascal_case("set_threshold"), "SetThreshold");
    assert_eq!(to_pascal_case("http_request"), "HttpRequest");
    assert_eq!(to_pascal_case("send_http_request"), "SendHttpRequest");
    assert_eq!(to_pascal_case("a"), "A");
    assert_eq!(to_pascal_case("abc"), "Abc");
}
