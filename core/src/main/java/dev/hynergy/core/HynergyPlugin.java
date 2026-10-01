package dev.hynergy.core;

import com.hypixel.hytale.logger.HytaleLogger;
import com.hypixel.hytale.server.core.plugin.JavaPlugin;
import com.hypixel.hytale.server.core.plugin.JavaPluginInit;
import dev.hynergy.core.electricity.ElectricityModule;
import dev.hynergy.core.port.PortModule;
import lombok.Getter;
import org.jspecify.annotations.NonNull;

import java.util.ArrayList;
import java.util.List;
import java.util.logging.Level;

@SuppressWarnings("unused")
public class HynergyPlugin extends JavaPlugin {

    public static final @NonNull HytaleLogger LOGGER = HytaleLogger.forEnclosingClass();

    private static HynergyPlugin INSTANCE;

    private final @NonNull List<HynergyModule> modules = new ArrayList<>();
    @Getter private PortModule portModule;
    public ElectricityModule electricityModule;

    public HynergyPlugin(JavaPluginInit init) {
        super(init);
        INSTANCE = this;
    }

    public static HynergyPlugin get() {
        return INSTANCE;
    }

    @Override
    protected void setup() {
        LOGGER.at(Level.INFO).log("Setting up Hynergy!");

        this.portModule = registerModule(new PortModule());
        this.electricityModule = registerModule(new ElectricityModule(portModule, getChunkStoreRegistry()));

        for (HynergyModule module : modules) {
            module.setup();
        }
    }

    @Override
    protected void start() {
        super.start();
        LOGGER.at(Level.INFO).log("Starting Hynergy!");

        for (HynergyModule module : modules) {
            module.start();
        }
    }

    @Override
    protected void shutdown() {
        super.shutdown();

        LOGGER.at(Level.INFO).log("Shutting down Hynergy!");

        for (int index = modules.size() - 1; index >= 0; index--) {
            modules.get(index).shutdown();
        }
    }

    private <T extends HynergyModule> T registerModule(@NonNull T module) {
        modules.add(module);
        return module;
    }
}
