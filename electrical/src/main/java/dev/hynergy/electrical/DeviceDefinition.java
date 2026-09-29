package dev.hynergy.electrical;

public class DeviceDefinition {
    private final int id;

    DeviceDefinition(int id) {
        if (id == 0) {
            throw new IllegalArgumentException("Definition ID must be non-zero");
        }

        this.id = id;
    }

    int id() {
        return id;
    }
}
