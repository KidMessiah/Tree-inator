"""Just enough of Blender's bpy for treegen/parametric/gen.py to build branch curves outside Blender.

Only the data the generator writes is kept: per level, a list of Bezier splines, each a list of points with
co, handle_left, handle_right and radius.
"""
from mathutils import Vector


class _Point:
    __slots__ = ("co", "handle_left", "handle_right", "radius")

    def __init__(self):
        self.co = Vector()
        self.handle_left = Vector()
        self.handle_right = Vector()
        self.radius = 1.0


class _Points(list):
    def add(self, n):
        for _ in range(n):
            self.append(_Point())


class _Spline:
    def __init__(self):
        self.bezier_points = _Points([_Point()])
        self.resolution_u = 1
        self.radius_interpolation = "CARDINAL"


class _Splines(list):
    def new(self, kind):
        s = _Spline()
        self.append(s)
        return s


class _Curve:
    def __init__(self, name=""):
        self.name = name
        self.splines = _Splines()
        self.dimensions = "3D"
        self.resolution_u = 1
        self.fill_mode = "FULL"
        self.bevel_depth = 1
        self.bevel_resolution = 0


class _Obj:
    def __init__(self, name="", data=None):
        self.name = name
        self.data = data
        self.parent = None


class _Factory:
    def __init__(self, cls):
        self.cls = cls

    def new(self, name, *args, **kwargs):
        return self.cls(name, *args) if self.cls is _Obj else self.cls(name)


class _Links:
    def link(self, obj):
        pass


class _Collection:
    objects = _Links()


class _Data:
    curves = _Factory(_Curve)
    objects = _Factory(_Obj)


class _Context:
    collection = _Collection()


data = _Data()
context = _Context()
