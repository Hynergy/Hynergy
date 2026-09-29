package dev.hynergy.electrical;

public final class ElectricSystem implements AutoCloseable {
    private final ElectricalRuntime runtime;
    private final ElectricalWorld world;

    private boolean closed;


    ElectricSystem(ElectricalRuntime runtime, ElectricalWorld world) {
        this.runtime = runtime;
        this.world = world;
    }



    public Wire createWire() {
        requireOpen();

        int id = world.addWire();
        int generation = world.wireGeneration(id);

        return new Wire(this, id, generation);
    }

    void connect(Wire first, Wire second) {
        requireOwned(first);
        requireOwned(second);

        world.connectWires(first.id(), first.generation(), second.id(), second.generation());
    }

    void disconnect(Wire first, Wire second) {
        requireOwned(first);
        requireOwned(second);

        world.disconnectWires(first.id(), first.generation(), second.id(), second.generation());
    }

    void remove(Wire wire) {
        requireOwned(wire);

        world.removeWire(wire.id(), wire.generation());
    }



    public <T extends Device> T create(DeviceType<T> type) {
        requireOpen();

        DeviceDefinition definition = runtime.requireDefinition(type);

        T device = type.construct();

        int id = world.addDevice(definition);
        int generation = world.deviceGeneration(id);

        device.bind(this, id, generation);

        return device;
    }

    void setParameter(Device device, int parameterId, double value) {
        requireOwned(device);

        world.setDeviceParameter(device.id(), device.generation(), parameterId, value);
    }

    void attachTerminal(Device device, int terminalId, Wire wire) {
        requireOwned(device);
        requireOwned(wire);

        world.attachTerminal(wire.id(), wire.generation(), device.id(), device.generation(), terminalId);
    }

    void detachTerminal(Device device, int terminalId, Wire wire) {
        requireOwned(device);
        requireOwned(wire);

        world.detachTerminal(wire.id(), wire.generation(), device.id(), device.generation(), terminalId);
    }

    void remove(Device device) {
        requireOwned(device);

        world.removeDevice(device.id(), device.generation());
    }



    private void requireOwned(Wire wire) {
        requireOpen();

        if (!wire.belongsTo(this)) {
            throw new IllegalArgumentException("Wire belongs to another electrical system");
        }
    }

    private void requireOwned(Device device) {
        requireOpen();

        if (!device.belongsTo(this)) {
            throw new IllegalArgumentException("Device belongs to another electrical system");
        }
    }

    private void requireOpen() {
        if (closed) {
            throw new IllegalStateException("Electrical system is closed");
        }
    }

    @Override
    public void close() {
        if (closed) {
            return;
        }

        world.close();
        closed = true;

        runtime.releaseSystem();
    }
}