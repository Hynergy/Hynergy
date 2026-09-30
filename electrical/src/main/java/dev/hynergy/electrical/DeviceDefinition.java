package dev.hynergy.electrical;

import lombok.AccessLevel;
import lombok.Getter;
import lombok.Value;

/**
 * Identifies a registered electrical device definition.
 *
 * <p>The electrical runtime creates this object during device type
 * registration. Plugin code normally does not need to inspect it.</p>
 */
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
