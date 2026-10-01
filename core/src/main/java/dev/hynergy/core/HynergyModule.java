package dev.hynergy.core;

public abstract class HynergyModule {
    protected final static HynergyPlugin HYNERGY_PLUGIN = HynergyPlugin.get();

    protected abstract void setup();

    protected void start() {
    }

    protected void shutdown() {
    }
}