using System.Text.Json.Nodes;

namespace json;

public static class Bench
{
    static JsonObject Record(int i) => new JsonObject
    {
        ["id"] = i,
        ["name"] = "user" + i,
        ["active"] = i % 3 == 0,
        ["tags"] = new JsonArray("t" + (i % 10), "x"),
        ["pos"] = new JsonObject { ["x"] = i % 100, ["y"] = i % 37 },
    };

    public static void Run(string[] args)
    {
        int n = int.Parse(args[0]);
        var root = new JsonArray();
        for (int i = 0; i < n; i++) root.Add(Record(i));
        string text = root.ToJsonString();
        var back = JsonNode.Parse(text).AsArray();
        long acc = text.Length;
        foreach (var r in back)
        {
            acc += (long)r["id"] + (long)r["pos"]["y"];
            if ((bool)r["active"]) acc += 1;
        }
        Console.WriteLine("RESULT " + acc);
    }
}
