// Original synthetic fixture generator (JDK 21).
// Run from this directory: TZ=UTC java RuntimePackageFixture.java
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;

class RuntimePackageFixture {
    static final String PROJECT = """
            {"schemaVersion":1,"metadata":{"name":"Synthetic café","defaultScreenId":"main"},"screens":{"main":{"id":"main","title":"Main","widthDp":640,"heightDp":480,"components":{}}},"variables":{},"externalSources":{},"externalMappings":{}}
            """.stripTrailing();
    static final String SETTINGS = """
            {"values":"SIMULATED","fullScreen":true,"statusBar":true,"keepScreenOn":true}
            """.stripTrailing();
    static byte[] utf8(String value) {
        return value.getBytes(StandardCharsets.UTF_8);
    }
    static String sha(String value) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(utf8(value)));
    }
    static String manifest(int format, String id) throws Exception {
        String body = """
                {
                    "format": %d,
                    "packageId": "%s",
                    "projectName": "Synthetic café",
                    "version": "1.0",
                    "schemaVersion": 1,
                    "createdAtMs": 1000,
                    "createdBy": "synthetic",
                    "startupScreen": "main",
                    "files": [
                        {
                            "path": "project.json",
                            "sha256": "%s",
                            "sizeBytes": %d
                        },
                        {
                            "path": "runtime.json",
                            "sha256": "%s",
                            "sizeBytes": %d
                        }
                    ]%s
                }
                """;
        String signing = format == 2 ? ",\n    \"sequence\": 7,\n    \"publisherKeyId\": \"synthetic-key\"" : "";
        return body.formatted(format, id, sha(PROJECT), utf8(PROJECT).length,
                sha(SETTINGS), utf8(SETTINGS).length, signing).stripTrailing();
    }
    static void entry(ZipOutputStream zip, String name, String content) throws Exception {
        var entry = new ZipEntry(name);
        entry.setTime(315532800000L);
        zip.putNextEntry(entry);
        zip.write(utf8(content));
        zip.closeEntry();
    }
    public static void main(String[] args) throws Exception {
        for (int format : new int[]{1, 2}) {
            String id = format == 1
                    ? sha("project.json:" + sha(PROJECT) + ":" + utf8(PROJECT).length
                        + "\nruntime.json:" + sha(SETTINGS) + ":" + utf8(SETTINGS).length)
                    : sha(manifest(format, ""));
            var path = Path.of(format == 1 ? "runtime-unsigned-java.synthetic" : "runtime-signed-java.synthetic");
            try (var zip = new ZipOutputStream(Files.newOutputStream(path))) {
                entry(zip, "manifest.json", manifest(format, id));
                entry(zip, "project.json", PROJECT);
                entry(zip, "runtime.json", SETTINGS);
                // Deliberately fake signature: verify metadata only, never trust.
                if (format == 2) entry(zip, "signature.json",
                    "{\"algorithm\":\"TEST\",\"keyId\":\"synthetic-key\",\"value\":\"AQID\"}");
            }
            System.out.println(path + " " + id);
        }
    }
}
