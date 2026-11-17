extends SceneTree

const WARMUP := 3
const SAMPLES := 9
const SIZES := [64, 4096, 16384]
const DICTIONARIES := "generic_dictionaries_one_node"
const PACKED := "native_variant_packed_int32_one_node"
const TILE_MAP := "native_tile_map_layer_restricted"
const MODES := [DICTIONARIES, PACKED, TILE_MAP]


func _initialize() -> void:
	_run.call_deferred()


func _check(condition: bool, message: String) -> bool:
	if not condition:
		printerr("Runtime scene comparison failed: " + message)
		quit(1)
	return condition


@warning_ignore("integer_division")
func _rows(count: int, side: int) -> Array[Dictionary]:
	var rows: Array[Dictionary] = []
	for i in range(count):
		rows.append({"x": i % side, "y": i / side, "tile": i % 64, "flags": i % 4})
	return rows


func _save_scene(mode: String, rows: Array[Dictionary], path: String) -> bool:
	var root_node: Node = TileMapLayer.new() if mode == TILE_MAP else Node.new()
	root_node.name = "Terrain"
	if mode == DICTIONARIES:
		root_node.set_meta("cells", rows)
	elif mode == PACKED:
		var cells := PackedInt32Array()
		cells.resize(rows.size() * 4)
		for i in range(rows.size()):
			var row: Dictionary = rows[i]
			cells[i * 4] = row.x
			cells[i * 4 + 1] = row.y
			cells[i * 4 + 2] = row.tile
			cells[i * 4 + 3] = row.flags
		root_node.set_meta("cells", cells)
	else:
		var layer := root_node as TileMapLayer
		for row in rows:
			layer.set_cell(Vector2i(row.x, row.y), 0, Vector2i(row.tile, 0), row.flags)
	var scene := PackedScene.new()
	var result := scene.pack(root_node)
	root_node.free()
	if not _check(result == OK, "PackedScene.pack: " + path):
		return false
	return _check(ResourceSaver.save(scene, path) == OK, "ResourceSaver.save: " + path)


func _validate(root_node: Node, mode: String, rows: Array[Dictionary], expected: int) -> bool:
	if not _check(root_node.get_child_count() == 0 and root_node.get_script() == null,
			"expected one script-free root"):
		return false
	var checksum: int = 0
	if mode == TILE_MAP:
		if not _check(root_node is TileMapLayer, "expected TileMapLayer root"):
			return false
		var layer := root_node as TileMapLayer
		if not _check(layer.tile_set == null and layer.get_used_cells().size() == rows.size(),
				"native used-cell count or unexpected TileSet"):
			return false
		if not _check(layer.tile_map_data.size() == 2 + rows.size() * 12,
				"expected a 2-byte header and 12 serialized bytes per native cell"):
			return false
		for row in rows:
			var coords := Vector2i(row.x, row.y)
			var atlas := layer.get_cell_atlas_coords(coords)
			var flags := layer.get_cell_alternative_tile(coords)
			if not _check(layer.get_cell_source_id(coords) == 0
					and atlas == Vector2i(row.tile, 0) and flags == row.flags,
					"native cell differs at " + str(coords)):
				return false
			checksum += coords.x + coords.y + atlas.x + flags
	else:
		var cells: Variant = root_node.get_meta("cells", null)
		if mode == DICTIONARIES:
			if not _check(cells is Array and cells.size() == rows.size(), "dictionary row count"):
				return false
			for i in range(rows.size()):
				if not _check(cells[i] is Dictionary and cells[i] == rows[i],
						"dictionary row differs at " + str(i)):
					return false
				checksum += cells[i].x + cells[i].y + cells[i].tile + cells[i].flags
		else:
			if not _check(cells is PackedInt32Array and cells.size() == rows.size() * 4,
					"packed value count"):
				return false
			for i in range(rows.size()):
				var row: Dictionary = rows[i]
				var offset := i * 4
				if not _check(cells[offset] == row.x and cells[offset + 1] == row.y
						and cells[offset + 2] == row.tile and cells[offset + 3] == row.flags,
						"packed row differs at " + str(i)):
					return false
				checksum += cells[offset] + cells[offset + 1] + cells[offset + 2] + cells[offset + 3]
	return _check(checksum == expected, "checksum differs: " + mode)


func _benchmark(mode: String, rows: Array[Dictionary], side: int, path: String) -> Dictionary:
	var source_bytes := FileAccess.get_file_as_bytes(path).size()
	if not _check(source_bytes > 0, "empty or unreadable scene: " + path):
		return {}
	var expected: int = 0
	for row in rows:
		expected += row.x + row.y + row.tile + row.flags
	var reads: Array[int] = []
	var utf8_decodes: Array[int] = []
	var decodes: Array[int] = []
	var instantiates: Array[int] = []
	for sample in range(WARMUP + SAMPLES):
		var start := Time.get_ticks_usec()
		var bytes := FileAccess.get_file_as_bytes(path)
		var read_ns := (Time.get_ticks_usec() - start) * 1000
		start = Time.get_ticks_usec()
		var text := bytes.get_string_from_utf8()
		var utf8_ns := (Time.get_ticks_usec() - start) * 1000
		start = Time.get_ticks_usec()
		var scene := ResourceLoader.load(path, "PackedScene", ResourceLoader.CACHE_MODE_IGNORE) as PackedScene
		var decode_ns := (Time.get_ticks_usec() - start) * 1000
		if not _check(scene != null, "ResourceLoader.load: " + path):
			return {}
		start = Time.get_ticks_usec()
		var root_node := scene.instantiate()
		var instantiate_ns := (Time.get_ticks_usec() - start) * 1000
		if not _check(root_node != null, "PackedScene.instantiate: " + path):
			return {}
		var valid := _check(bytes.size() == source_bytes and text.begins_with("[gd_scene"),
				"file read or UTF-8 decode differs: " + path)
		if valid:
			valid = _validate(root_node, mode, rows, expected)
		root_node.free()
		scene = null
		text = ""
		bytes.clear()
		if not valid:
			return {}
		if sample >= WARMUP:
			reads.append(read_ns)
			utf8_decodes.append(utf8_ns)
			decodes.append(decode_ns)
			instantiates.append(instantiate_ns)
	return {
		"engine": "godot", "engine_version": Engine.get_version_info(), "format": "tscn",
		"mode": mode, "rows": rows.size(), "side": side, "nodes": 1,
		"source_file": path, "source_bytes": source_bytes, "checksum": expected,
		"warmup": WARMUP, "samples": SAMPLES, "timer_resolution_ns": 1000,
		"warm_read_ns": reads, "utf8_decode_ns": utf8_decodes,
		"decode_ns": decodes, "instantiate_ns": instantiates,
		"warm_read_scope": "FileAccess.get_file_as_bytes; open + read + close; warm OS cache",
		"utf8_decode_scope": "PackedByteArray.get_string_from_utf8; no file I/O",
		"decode_scope": "ResourceLoader.load PackedScene CACHE_MODE_IGNORE; includes fresh file read and UTF-8/text parsing",
		"instantiate_scope": "PackedScene.instantiate root outside SceneTree; no scripts, TileSet, rendering or tree entry",
		"native_cell_store_bytes": 2 + rows.size() * 12 if mode == TILE_MAP else null,
		"used_cells": rows.size() if mode == TILE_MAP else null,
		"state_verified": true,
	}


func _run() -> void:
	var args := OS.get_cmdline_user_args()
	if not _check(args.size() == 1, "usage: --headless --path <fixture> --script main.gd -- <owned-output-directory>"):
		return
	if not _check(DisplayServer.get_name() == "headless", "run with --headless"):
		return
	var directory := ProjectSettings.globalize_path(args[0])
	if not _check(directory.is_absolute_path(), "output directory must be absolute or user://"):
		return
	if not _check(DirAccess.make_dir_recursive_absolute(directory) == OK, "create output directory"):
		return
	for count in SIZES:
		var side := int(sqrt(count))
		if not _check(side * side == count, "population must be square"):
			return
		var rows := _rows(count, side)
		for mode in MODES:
			var path := directory.path_join("godot-%s-%d.tscn" % [mode, count])
			if not _save_scene(mode, rows, path):
				return
			var record := _benchmark(mode, rows, side, path)
			if record.is_empty():
				return
			print(JSON.stringify(record))
	quit(0)
