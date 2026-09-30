package dev.hynergy.core;

import com.hypixel.hytale.logger.HytaleLogger;
import com.hypixel.hytale.server.core.plugin.JavaPlugin;
import com.hypixel.hytale.server.core.plugin.JavaPluginInit;
import dev.hynergy.core.electricity.ElectricityModule;
import org.jspecify.annotations.Nullable;

import java.util.ArrayList;
import java.util.List;
import java.util.logging.Level;

public class HynergyPlugin extends JavaPlugin {

    public static final HytaleLogger LOGGER = HytaleLogger.forEnclosingClass();

    private final List<HynergyModule> modules = new ArrayList<>();
    public @Nullable ElectricityModule electricityModule;

    public HynergyPlugin(JavaPluginInit init) {
        super(init);
    }


    @Override
    protected void setup() {
        LOGGER.at(Level.INFO).log("Setting up Hynergy!");

        this.electricityModule = registerModule(new ElectricityModule(getChunkStoreRegistry()));
        for (HynergyModule module : modules) {
            module.setup();
        }
    }

    @Override
    protected void start() {
        LOGGER.at(Level.INFO).log("Starting Hynergy!");
        for (HynergyModule module : modules) {
            module.start();
        }
    }

    @Override
    protected void shutdown() {
        LOGGER.at(Level.INFO).log("Shutting down Hynergy!");

        for (int index = modules.size() - 1; index >= 0; index--) {
            modules.get(index).shutdown();
        }
    }

    private <T extends HynergyModule> T registerModule(T module) {
        modules.add(module);
        return module;
    }
}
