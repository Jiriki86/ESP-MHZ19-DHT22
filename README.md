# MH-Z19 CO2 and DHT-22 temperature sensors using Rust

This little side project reads the CO2 concentration in my bedroom as well as the ambient temperature and humidity. The collected data is then send to an
MQTT broker running on my raspberrypi.

## Hardware Setup

The project uses an ESP32 development kit (v1) which is programmed using rust. The CO2 sensor is readout using its serial interface using the GPIO pins
32 and 33. The DHT-22 uses a single data line to request and receive data and is connected to GPIO pin 4.

## Configuration file

To compile and run the project you will need to place a configuration file cfg.toml with your wifi setup in the root directory. The file should have 
the following content


    [co2-sensor]
    wifi_ssid = "<wifi-ssid>"
    wifi_psk = "<wifi-password>"
    mqtt_host = "<host-address-of-mqtt-broker>"
    mqtt_user = "<mqtt-username>"
    mqtt_pass = "<mqtt-passwor>"
