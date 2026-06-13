(goal
  (fp.gt x0 (_ +zero 8 24))
  (fp.gt x1 (_ +zero 8 24))
  (fp.gt x2 (_ +zero 8 24))
  (fp.gt x3 (_ +zero 8 24))
  (fp.gt x4 (_ +zero 8 24))
  (fp.gt x5 (_ +zero 8 24))
  (fp.gt x6 (_ +zero 8 24))
  (fp.gt x7 (_ +zero 8 24))
  (fp.gt x8 (_ +zero 8 24))
  (fp.gt x9 (_ +zero 8 24))
  (fp.gt x10 (_ +zero 8 24))
  (fp.gt x11 (_ +zero 8 24))
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
    (fp.gt (fp.add roundNearestTiesToEven
                   (fp.add roundNearestTiesToEven
                           (fp.add roundNearestTiesToEven a!2 x9)
                           x10)
                   x11)
           (_ +zero 8 24)))))