#!/usr/bin/env python3

"""Modules for initilizing, reading, writing, updatingand testing
config.ini file."""

import configparser
# from rich import print

config = configparser.ConfigParser()
config.read("config.ini")

DEFAULT_CONFIG = {
    "regenerate": {
        "inflections": "yes",
        "transliterations": "yes",
        "freq_maps": "yes"
    },
    "deconstructor": {
        "include_cloud": "no"
    },
    "gui": {
        "theme": "DarkGrey10",
        "screen_fraction_width": "0.60",
        "screen_fraction_height": "1",
        "window_x": "0",
        "window_y": "0",
        "font_name": "Noto Sans",
        "font_size": "14",
        "input_text_color": "darkgray",
        "text_color": "#00bfff",
        "element_padding_x": "0",
        "element_padding_y": "0",
        "margin_x": "0",
        "margin_y": "0"
    },
    "goldendict": {
        "copy_unzip": "yes",
        "path": ""
    },
    "dictionary": {
        "make_mdict": "yes",
        "link_url": "https://www.thebuddhaswords.net/",
        "make_link": "no",
        "extended_synonyms": "no"
    },
    "exporter" : {
        "make_ebook": "no",
        "make_tpr": "no",
    },
    "openia": {
        "key": ""
    }
}


def config_initialize() -> None:
    """Initialize config.ini with default values."""
    for section, options in DEFAULT_CONFIG.items():
        if not config.has_section(section):
            config.add_section(section)
        for option, value in options.items():
            if not config.has_option(section, option):
                config.set(section, option, value)


def config_read(section: str, option: str, default_value=None) -> (str):
    """Read config.ini. If error, return a specified default value"""
    try:
        return config.get(section, option)
    except (configparser.NoSectionError, configparser.NoOptionError):
        return ""


def config_write() -> None:
    """Write config.ini."""
    with open("config.ini", "w") as file:
        config.write(file)


def config_update(section: str, option: str, value) -> None:
    """Update config.ini with a new section, option & value."""
    if config.has_section(section):
        config.set(section, option, str(value))
    else:
        config.add_section(section)
        config.set(section, option, str(value))
    config_write()
    # print(f"[green]config setting updated: '{section}, {option}' is '{value}'")


def config_test(section: str, option: str, value) -> bool:
    """Test config.ini to see if a section, option equals a value."""
    if config.has_section(section) and config.has_option(section, option):
        current_value = config.get(section, option)
        # print(f"current config setting: '{section}, {option}' is '{current_value}'")
        return config.get(section, option) == str(value)
    else:
        # print(f"[yellow]unknown config setting: [brightyellow]'{section}, {option}'")
        config_update_default_value(section, option)
        return config.get(section, option, fallback='') == str(value)


def config_update_default_value(section: str, option: str) -> None:
    """Update config.ini with a default value for a missing section or option."""
    if section in DEFAULT_CONFIG and option in DEFAULT_CONFIG[section]:
        default_value = DEFAULT_CONFIG[section].get(option)
        config_update(section, option, default_value)
    else:
        pass
        # print(f"[red]missing default value for option: [brightyellow]{section}, {option}")


def config_test_section(section):
    """Test config.ini to see if a section exists."""
    if config.has_section(section):
        return True
    else:
        return False

def config_test_option(section, option):
    """Test config.ini to see if a section, option exists."""
    if config.has_section(section):
        return config.has_option(section, option)
    else:
        return False


if __name__ == "__main__":
    config_initialize()
