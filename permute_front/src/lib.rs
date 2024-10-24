//! Here is frontend of the framework. All configuration is registered here.

/// YAML file configurations.
pub mod yaml;

/// Context with all registered configurations.
pub mod context;

#[cfg(test)]
fn setup_logger() {
    use std::sync::OnceLock;
    static START_TIME: OnceLock<std::time::Instant> = OnceLock::new();

    // Allow to try to run multiple times. It will only run once anyway.
    let _ = START_TIME.get_or_init(|| {
        fern::Dispatch::new()
            .format(|out, message, record| {
                let colors = fern::colors::ColoredLevelConfig::new()
                    .trace(fern::colors::Color::Magenta)
                    .debug(fern::colors::Color::Blue)
                    .info(fern::colors::Color::Green)
                    .warn(fern::colors::Color::Yellow)
                    .error(fern::colors::Color::Red);

                out.finish(format_args!(
                    "{} [{}] {}",
                    humantime::format_duration(START_TIME.get().unwrap().elapsed()),
                    colors.color(record.level()),
                    message
                ))
            })
            .level(log::LevelFilter::Trace)
            .chain(std::io::stdout())
            .apply()
            .unwrap();

        std::time::Instant::now()
    });
}
