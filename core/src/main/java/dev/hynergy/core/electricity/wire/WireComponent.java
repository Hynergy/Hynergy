package dev.hynergy.core.electricity.wire;

import com.hypixel.hytale.codec.Codec;
import com.hypixel.hytale.codec.KeyedCodec;
import com.hypixel.hytale.codec.builder.BuilderCodec;
import com.hypixel.hytale.codec.schema.metadata.ui.UIEditor;
import com.hypixel.hytale.codec.validation.Validators;
import com.hypixel.hytale.component.Component;
import com.hypixel.hytale.server.core.universe.world.storage.ChunkStore;
import dev.hynergy.core.electricity.ElectricalCodecs;
import dev.hynergy.electrical.Wire;
import dev.hynergy.electrical.WireId;
import lombok.Getter;
import lombok.NoArgsConstructor;
import lombok.Setter;
import org.jspecify.annotations.Nullable;

@NoArgsConstructor
public class WireComponent implements Component<ChunkStore> {

    public static final BuilderCodec<WireComponent> CODEC =
            BuilderCodec.builder(WireComponent.class, WireComponent::new)
                        .append(
                                new KeyedCodec<>("Config", Codec.STRING),
                                WireComponent::setConfigId,
                                WireComponent::getConfigId
                        )
                        .metadata(
                                new UIEditor(
                                        new UIEditor.Dropdown(
                                                WireConfig.DATA_SET
                                        )
                                )
                        )
                        .addValidator(Validators.nonNull())
                        .addValidator(Validators.nonEmptyString())
                        .addValidatorLate(
                                () -> WireConfig.VALIDATOR_CACHE
                                        .getValidator()
                                        .late()
                        )
                        .addValidatorLate(
                                () -> WireConfig.VALIDATOR_CACHE
                                        .getValidator()
                                        .late()
                        )
                        .add()
                        .append(
                                new KeyedCodec<>("WireId", ElectricalCodecs.WIRE_ID),
                                (component, id) -> component.wireId = id,
                                component -> component.wireId
                        )
                        .add()
                        .build();

    @Getter @Setter private String configId;
    @Getter @Setter private @Nullable WireId wireId;
    @Getter @Setter private transient @Nullable Wire wire;

    private WireComponent(String configId) {
        this.configId = configId;
    }

    @Override
    public Component<ChunkStore> cloneSerializable() {
        WireComponent cloned = new WireComponent(configId);
        cloned.wireId = wireId;
        return cloned;
    }

    @Override
    public @Nullable Component<ChunkStore> clone() {
        try {
            WireComponent cloned = (WireComponent) super.clone();

            cloned.wireId = null;
            cloned.wire = null;

            return cloned;
        } catch (CloneNotSupportedException e) {
            throw new AssertionError(e);
        }
    }
}
