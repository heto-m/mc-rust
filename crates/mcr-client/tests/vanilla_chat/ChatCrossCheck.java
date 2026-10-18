import com.mojang.authlib.GameProfile;
import io.netty.buffer.Unpooled;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.KeyFactory;
import java.security.PrivateKey;
import java.security.PublicKey;
import java.security.spec.PKCS8EncodedKeySpec;
import java.security.spec.X509EncodedKeySpec;
import java.time.Instant;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.List;
import java.util.UUID;
import net.minecraft.commands.arguments.ArgumentSignatures;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.chat.LastSeenMessages;
import net.minecraft.network.chat.LastSeenMessagesValidator;
import net.minecraft.network.chat.MessageSignature;
import net.minecraft.network.chat.RemoteChatSession;
import net.minecraft.network.chat.SignedMessageBody;
import net.minecraft.network.chat.SignedMessageChain;
import net.minecraft.network.protocol.game.ServerboundChatAckPacket;
import net.minecraft.network.protocol.game.ServerboundChatCommandSignedPacket;
import net.minecraft.network.protocol.game.ServerboundChatPacket;
import net.minecraft.network.protocol.game.ServerboundChatSessionUpdatePacket;
import net.minecraft.util.SignatureValidator;
import net.minecraft.util.Signer;

/**
 * Plays mcr's serverbound chat through vanilla 26.3's own server-side checks, and signs messages with
 * vanilla's chain for mcr to verify. Input and output are line-based: a verb, then hex or text fields.
 */
public final class ChatCrossCheck {
    private static final HexFormat HEX = HexFormat.of();

    public static void main(String[] args) throws Exception {
        List<String> out = new ArrayList<>();
        KeyFactory rsa = KeyFactory.getInstance("RSA");
        SignatureValidator services = null;
        UUID profile = null;
        RemoteChatSession session = null;
        SignedMessageChain.Decoder decoder = null;
        LastSeenMessagesValidator validator = new LastSeenMessagesValidator(20);
        PrivateKey signingKey = null;
        int checks = 0;

        for (String line : Files.readAllLines(Path.of(args[0]))) {
            String[] f = line.split(" ", -1);
            switch (f[0]) {
                case "services" -> {
                    PublicKey key = rsa.generatePublic(new X509EncodedKeySpec(HEX.parseHex(f[1])));
                    services = SignatureValidator.from(key, "SHA1withRSA");
                }
                case "profile" -> profile = UUID.fromString(f[1]);
                case "session" -> {
                    ServerboundChatSessionUpdatePacket packet =
                        ServerboundChatSessionUpdatePacket.STREAM_CODEC.decode(Unpooled.wrappedBuffer(HEX.parseHex(f[1])));
                    session = packet.chatSession().validate(new GameProfile(profile, "Player"), services);
                    decoder = session.createMessageDecoder(profile);
                    checks++;
                }
                case "bad_session" -> {
                    ServerboundChatSessionUpdatePacket packet =
                        ServerboundChatSessionUpdatePacket.STREAM_CODEC.decode(Unpooled.wrappedBuffer(HEX.parseHex(f[1])));
                    try {
                        packet.chatSession().validate(new GameProfile(profile, "Player"), services);
                        throw new IllegalStateException("a session for another profile validated");
                    } catch (net.minecraft.world.entity.player.ProfilePublicKey.ValidationException expected) {
                        checks++;
                    }
                }
                case "recv" -> validator.addPending(new MessageSignature(HEX.parseHex(f[1])));
                case "chat" -> {
                    FriendlyByteBuf buf = new FriendlyByteBuf(Unpooled.wrappedBuffer(HEX.parseHex(f[1])));
                    ServerboundChatPacket packet = ServerboundChatPacket.STREAM_CODEC.decode(buf);
                    require(buf.readableBytes() == 0, "trailing chat bytes");
                    LastSeenMessages lastSeen = validator.applyUpdate(packet.lastSeenMessages());
                    require(packet.message().equals(text(f[2])), "chat text");
                    decoder.unpack(
                        packet.signature().orElse(null),
                        new SignedMessageBody(packet.message(), packet.timeStamp(), packet.salt(), lastSeen)
                    );
                    checks++;
                }
                case "command" -> {
                    ServerboundChatCommandSignedPacket packet =
                        ServerboundChatCommandSignedPacket.STREAM_CODEC.decode(Unpooled.wrappedBuffer(HEX.parseHex(f[1])));
                    LastSeenMessages lastSeen = validator.applyUpdate(packet.lastSeenMessages());
                    String value = text(f[3]);
                    require(packet.argumentSignatures().entries().size() == 1, "one argument signature");
                    ArgumentSignatures.Entry entry = packet.argumentSignatures().entries().get(0);
                    require(entry.name().equals(f[2]), "argument name");
                    decoder.unpack(entry.signature(), new SignedMessageBody(value, packet.timeStamp(), packet.salt(), lastSeen));
                    checks++;
                }
                case "ack" -> {
                    FriendlyByteBuf buf = new FriendlyByteBuf(Unpooled.wrappedBuffer(HEX.parseHex(f[1])));
                    validator.applyOffset(ServerboundChatAckPacket.STREAM_CODEC.decode(buf).offset());
                    checks++;
                }
                case "forged" -> {
                    FriendlyByteBuf buf = new FriendlyByteBuf(Unpooled.wrappedBuffer(HEX.parseHex(f[1])));
                    ServerboundChatPacket packet = ServerboundChatPacket.STREAM_CODEC.decode(buf);
                    LastSeenMessages lastSeen = validator.applyUpdate(packet.lastSeenMessages());
                    SignedMessageChain.Decoder fresh = session.createMessageDecoder(profile);
                    try {
                        fresh.unpack(
                            packet.signature().orElse(null),
                            new SignedMessageBody(packet.message() + "!", packet.timeStamp(), packet.salt(), lastSeen)
                        );
                        throw new IllegalStateException("a tampered message verified");
                    } catch (SignedMessageChain.DecodeException expected) {
                        checks++;
                    }
                }
                case "checksum" -> {
                    List<MessageSignature> entries = new ArrayList<>();
                    for (int i = 2; i < f.length; i++) {
                        entries.add(new MessageSignature(HEX.parseHex(f[i])));
                    }
                    int expected = Integer.parseInt(f[1]);
                    require((new LastSeenMessages(entries).computeChecksum() & 0xff) == expected, "checksum " + expected);
                    checks++;
                }
                case "signing_key" -> signingKey = rsa.generatePrivate(new PKCS8EncodedKeySpec(HEX.parseHex(f[1])));
                case "vanilla_sign" -> {
                    UUID sender = UUID.fromString(f[1]);
                    UUID sessionId = UUID.fromString(f[2]);
                    SignedMessageChain.Encoder encoder =
                        new SignedMessageChain(sender, sessionId).encoder(Signer.from(signingKey, "SHA256withRSA"));
                    long salt = Long.parseLong(f[3]);
                    Instant time = Instant.ofEpochMilli(Long.parseLong(f[4]));
                    String base = text(f[5]);
                    List<MessageSignature> seen = new ArrayList<>();
                    for (int i = 6; i < f.length; i++) {
                        seen.add(new MessageSignature(HEX.parseHex(f[i])));
                    }
                    for (int index = 0; index < 2; index++) {
                        String message = base + index;
                        MessageSignature sig = encoder.pack(new SignedMessageBody(message, time, salt, new LastSeenMessages(seen)));
                        out.add("signed " + index + " " + HEX.formatHex(message.getBytes(StandardCharsets.UTF_8)) + " " + HEX.formatHex(sig.bytes()));
                    }
                }
                case "" -> {}
                default -> throw new IllegalArgumentException("unknown verb " + f[0]);
            }
        }
        out.add("checks " + checks);
        Files.write(Path.of(args[1]), out);
    }

    private static String text(String hex) {
        return new String(HEX.parseHex(hex), StandardCharsets.UTF_8);
    }

    private static void require(boolean ok, String what) {
        if (!ok) {
            throw new IllegalStateException("mismatch: " + what);
        }
    }
}
