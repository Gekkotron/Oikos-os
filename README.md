# Oikos-os

Oikos-os is the Rust home-automation hub that replaces Jeedom. It runs next to Mosquitto, owns the device registry built from Zigbee2MQTT and Home Assistant MQTT discovery, is the brain behind the Oikos Android app, and is configured through an MCP server so an AI assistant can inspect and change the house. The MCP server is served at `/mcp` and, like the REST API on the same port, is LAN-trusted with no auth in v1.

Status: scaffold
