(goal
  (fp.lt (_ +zero 8 24) x0)
  (fp.lt (_ +zero 8 24) x1)
  (fp.lt (_ +zero 8 24) x2)
  (fp.lt (_ +zero 8 24) x3)
  (fp.lt (_ +zero 8 24) x4)
  (fp.lt (_ +zero 8 24) x5)
  (fp.lt (_ +zero 8 24) x6)
  (fp.lt (_ +zero 8 24) x7)
  (fp.lt (_ +zero 8 24) x8)
  (fp.lt (_ +zero 8 24) x9)
  (fp.lt (_ +zero 8 24) x10)
  (fp.lt (_ +zero 8 24) x11)
  (not (fp.isNaN x0))
  (not (fp.isNaN x1))
  (not (fp.isNaN x2))
  (not (fp.isNaN x3))
  (not (fp.isNaN x4))
  (not (fp.isNaN x5))
  (not (fp.isNaN x6))
  (not (fp.isNaN x7))
  (not (fp.isNaN x8))
  (not (fp.isNaN x9))
  (not (fp.isNaN x10))
  (not (fp.isNaN x11))
  (let ((a!1 (fp.add roundNearestTiesToEven
                     (fp.add roundNearestTiesToEven
                             (fp.add roundNearestTiesToEven
                                     (fp.add roundNearestTiesToEven x0 x1)
                                     x2)
                             x3)
                     x4)))
  (let ((a!2 (fp.add roundNearestTiesToEven
                     (fp.add roundNearestTiesToEven
                             (fp.add roundNearestTiesToEven
                                     (fp.add roundNearestTiesToEven a!1 x5)
                                     x6)
                             x7)
                     x8)))
    (fp.lt (_ +zero 8 24)
           (fp.add roundNearestTiesToEven
                   (fp.add roundNearestTiesToEven
                           (fp.add roundNearestTiesToEven a!2 x9)
                           x10)
                   x11)))))