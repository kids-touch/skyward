//! The built-in lightweight browser's navigation and UI contract.

use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::*;

    #[test]
    fn browser_is_reachable_from_advanced_tools_and_loads_typed_url() {
        let (fonts, info) = (fonts(), info());
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        assert!(app.frame(&fonts, &info).hits.iter().any(|hit| hit.action == Action::Open(Screen::Browser)));
        tap(&mut app, &fonts, &info, Action::Open(Screen::Browser));
        assert_eq!(app.screen(), Screen::Browser);
        app.state.browser_url = "https://example.com".into();
        assert_eq!(tap(&mut app, &fonts, &info, Action::GoBrowser), vec![Effect::Browse("https://example.com".into())]);
    }

    #[test]
    fn browser_links_navigate_to_the_extracted_target() {
        let (fonts, info) = (fonts(), info());
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        tap(&mut app, &fonts, &info, Action::Open(Screen::Browser));
        app.state.browser_page = Some(Ok(skyward_net::browser::BrowserPage {
            url: "https://example.com/".into(), title: "Example".into(), text: String::new(),
            links: vec![skyward_net::browser::BrowserLink { label: "Next".into(), url: "https://example.com/next".into() }],
            images: Vec::new(),
        }));
        assert_eq!(tap(&mut app, &fonts, &info, Action::BrowserLink(0)), vec![Effect::Browse("https://example.com/next".into())]);
        assert_eq!(app.state.browser_url, "https://example.com/next");
    }
    #[test]
    fn browser_back_and_forward_follow_navigation_history() {
        let (fonts, info) = (fonts(), info());
        let mut app = App::new(Screen::Advanced, W, H, PPD);
        tap(&mut app, &fonts, &info, Action::Open(Screen::Browser));
        app.state.browser_url = "https://example.com/one".into();
        assert_eq!(tap(&mut app, &fonts, &info, Action::GoBrowser), vec![Effect::Browse("https://example.com/one".into())]);
        app.state.browser_url = "https://example.com/two".into();
        assert_eq!(tap(&mut app, &fonts, &info, Action::GoBrowser), vec![Effect::Browse("https://example.com/two".into())]);
        assert_eq!(tap(&mut app, &fonts, &info, Action::BrowserBack), vec![Effect::Browse("https://example.com/one".into())]);
        assert_eq!(tap(&mut app, &fonts, &info, Action::BrowserForward), vec![Effect::Browse("https://example.com/two".into())]);
    }

}
