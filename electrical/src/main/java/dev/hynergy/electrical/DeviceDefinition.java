package dev.hynergy.electrical;

import lombok.AccessLevel;
import lombok.Getter;
import lombok.Value;

@Value
public class DeviceDefinition {
    @Getter(AccessLevel.NONE)
    int id;

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
